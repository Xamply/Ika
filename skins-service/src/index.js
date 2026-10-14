// Ika skins service (Cloudflare Worker + R2).
//
// Public skins by player name for offline accounts, without accounts or passwords:
//   GET    /skins/{name}.png   the skin (what SkinRestorer's "username-url" provider fetches)
//   PUT    /skins/{name}       upload/replace a skin   (signed, see below)
//   DELETE /skins/{name}       release the name        (signed)
//
// Mailbox between player names, for sending an instance to a friend PC to PC. The instance never
// passes through here: the two launchers only exchange an invitation and the few kilobytes they
// need to open a direct connection (WebRTC offer/answer).
//   GET    /share/box/{name}                  my messages; claims the name if it is free   (signed)
//   POST   /share/box/{to}?from={name}        leave a message {id, type, data} for {to}    (signed)
//   DELETE /share/box/{name}?keys=a,b         remove messages I have dealt with            (signed)
// Every stored message gets its own `key`: removing by key never takes a newer message of the
// same exchange that arrived in between.
//
// Ownership: the first device that uses a name owns it. Each launcher has an Ed25519 key bound
// to its PC (Windows DPAPI); the service only keeps the public key (with the skin, or in
// names/{name}.json when the name has no skin). Signed requests carry X-Ika-Pubkey /
// X-Ika-Timestamp / X-Ika-Signature. The signature is over
//   skins:   "ika-skin-v1\n{method}\n{name}\n{timestamp}\n{sha256(body) hex}"
//   mailbox: "ika-share-v1\n{method}\n{path and query}\n{timestamp}\n{sha256(body) hex}"

const NAME_RE = /^[A-Za-z0-9_]{2,16}$/;
const MAX_PNG_BYTES = 64 * 1024;
const MAX_CLOCK_SKEW_MS = 10 * 60 * 1000;

// Mailbox limits. An invitation waits for days; the connection handshake is only useful for
// minutes (both launchers must be open at the same time).
const SHARE_ID_RE = /^[A-Za-z0-9_-]{8,40}$/;
const SHARE_TTL_MS = { invite: 7 * 24 * 3600 * 1000, offer: 15 * 60 * 1000, answer: 15 * 60 * 1000, decline: 24 * 3600 * 1000, cancel: 24 * 3600 * 1000 };
const MAX_MESSAGE_BYTES = 24 * 1024;
const MAX_BOX_MESSAGES = 40;

const json = (status, data) =>
  new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json; charset=utf-8" } });

const b64ToBytes = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");

// Minecraft skins: PNG, 64x64 (or legacy 64x32).
function checkSkinPng(bytes) {
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (bytes.length < 33 || sig.some((b, i) => bytes[i] !== b)) return "not a PNG";
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const w = view.getUint32(16);
  const h = view.getUint32(20);
  if (w !== 64 || (h !== 64 && h !== 32)) return `skin must be 64x64 or 64x32, got ${w}x${h}`;
  return null;
}

// `scope` is what the signature covers besides method, time and body: the name for skins, the
// path and query for the mailbox (with its own prefix, so one can't be replayed as the other).
async function verifyRequest(request, prefix, method, scope, body) {
  const pubB64 = request.headers.get("x-ika-pubkey") || "";
  const ts = request.headers.get("x-ika-timestamp") || "";
  const sigB64 = request.headers.get("x-ika-signature") || "";
  if (!pubB64 || !ts || !sigB64) return { error: json(401, { error: "missing signature headers" }) };
  if (Math.abs(Date.now() - Number(ts)) > MAX_CLOCK_SKEW_MS) return { error: json(401, { error: "timestamp out of range" }) };
  let pub, sig;
  try {
    pub = b64ToBytes(pubB64);
    sig = b64ToBytes(sigB64);
  } catch {
    return { error: json(400, { error: "bad base64" }) };
  }
  if (pub.length !== 32 || sig.length !== 64) return { error: json(400, { error: "bad key or signature size" }) };
  const digest = hex(await crypto.subtle.digest("SHA-256", body));
  const message = new TextEncoder().encode(`${prefix}\n${method}\n${scope}\n${ts}\n${digest}`);
  const key = await crypto.subtle.importKey("raw", pub, { name: "Ed25519" }, false, ["verify"]);
  const ok = await crypto.subtle.verify({ name: "Ed25519" }, key, sig, message);
  if (!ok) return { error: json(401, { error: "invalid signature" }) };
  return { pubkey: pubB64 };
}

const ROOM_CODE_RE = /^\d{6}$/;
const ROOM_TTL_MS = 15 * 60 * 1000;
const MAX_ROOM_JSON_BYTES = 64 * 1024;
const MAX_CHUNK_BYTES = 4 * 1024 * 1024;

const skinKey = (name) => `skins/${name}.png`;
const nameKey = (name) => `names/${name}.json`;
const boxKey = (name) => `share/box/${name}.json`;
const roomKey = (code) => `share/room/${code}.json`;
const userRoomKey = (name) => `share/user-room/${String(name || "").trim().toLowerCase().replace(/^@/, "")}.json`;
const chunkKey = (code, idx) => `share/room/${code}/chunk-${idx}.bin`;

async function userToCode(name) {
  const clean = String(name || "").trim().toLowerCase().replace(/^@/, "");
  const data = new TextEncoder().encode(`ika-user-code:${clean}`);
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", data));
  const view = new DataView(hash.buffer, hash.byteOffset, hash.byteLength);
  const num = 100000 + (view.getUint32(0, false) % 900000);
  return String(num);
}

async function resolveUserRoom(env, rawUser) {
  const clean = String(rawUser || "").trim().toLowerCase().replace(/^@/, "");
  if (!clean) return { room: null, etag: null };
  try {
    const ptrObj = await env.SKINS.get(userRoomKey(clean));
    if (ptrObj) {
      const ptr = await ptrObj.json();
      if (ptr?.code && ROOM_CODE_RE.test(ptr.code)) {
        const found = await readRoom(env, ptr.code);
        if (found.room) return found;
      }
      await env.SKINS.delete(userRoomKey(clean));
    }
  } catch {}
  const derivedCode = await userToCode(clean);
  return readRoom(env, derivedCode);
}

// Public key of the device that owns the name, or null if nobody uses it yet.
async function ownerOf(env, name) {
  const skin = await env.SKINS.head(skinKey(name));
  if (skin?.customMetadata?.pubkey) return skin.customMetadata.pubkey;
  const record = await env.SKINS.get(nameKey(name));
  if (!record) return null;
  try {
    return (await record.json()).pubkey || null;
  } catch {
    return null;
  }
}

// The name must belong to this device; a free name becomes its own.
async function requireOwner(env, name, pubkey) {
  const owner = await ownerOf(env, name);
  if (owner && owner !== pubkey) return json(403, { error: "name owned by another device" });
  if (!owner) await env.SKINS.put(nameKey(name), JSON.stringify({ pubkey, claimed: Date.now() }));
  return null;
}

const alive = (message, now) => now - message.ts < (SHARE_TTL_MS[message.type] || 0);

async function readBox(env, name) {
  const obj = await env.SKINS.get(boxKey(name));
  if (!obj) return { messages: [], etag: null };
  let messages = [];
  try {
    messages = await obj.json();
  } catch {}
  return { messages: Array.isArray(messages) ? messages : [], etag: obj.etag };
}

// Read-modify-write of a mailbox; retried if someone else wrote it in between.
async function updateBox(env, name, change) {
  for (let attempt = 0; attempt < 8; attempt++) {
    if (attempt) await new Promise((resolve) => setTimeout(resolve, 20 + Math.random() * 60 * attempt));
    const { messages, etag } = await readBox(env, name);
    const now = Date.now();
    const next = change(messages.filter((m) => alive(m, now))).slice(-MAX_BOX_MESSAGES);
    if (!etag && next.length === 0) return true;
    const written = await env.SKINS.put(boxKey(name), JSON.stringify(next), etag ? { onlyIf: { etagMatches: etag } } : undefined);
    if (written) return true;
  }
  return false;
}

async function readRoom(env, code) {
  const obj = await env.SKINS.get(roomKey(code));
  if (!obj) return { room: null, etag: null };
  try {
    const room = await obj.json();
    if (!room || Date.now() - (room.updated || room.created || 0) > ROOM_TTL_MS) {
      await env.SKINS.delete(roomKey(code));
      if (room?.recipient) await env.SKINS.delete(userRoomKey(room.recipient));
      return { room: null, etag: null };
    }
    return { room, etag: obj.etag };
  } catch {
    await env.SKINS.delete(roomKey(code));
    return { room: null, etag: null };
  }
}

const publicRoom = (r) => ({
  code: r.code,
  id: r.id,
  sender: r.sender,
  recipient: r.recipient || null,
  meta: r.meta,
  offer: r.offer,
  answer: r.answer,
  mode: r.mode || "webrtc",
  state: r.state || "waiting",
  error: r.error || null,
  updated: r.updated,
});

async function handleRoom(request, env, url) {
  const method = request.method;

  // Binary chunk relay: /share/room/{code}/chunk/{idx}
  const chunkMatch = url.pathname.match(/^\/share\/room\/(\d{6})\/chunk\/(\d+)$/);
  if (chunkMatch) {
    const [, code, idxStr] = chunkMatch;
    const idx = Number(idxStr);
    if (!Number.isInteger(idx) || idx < 0 || idx > 100000) return json(400, { error: "invalid chunk index" });

    if (method === "PUT") {
      const body = new Uint8Array(await request.arrayBuffer());
      if (body.length > MAX_CHUNK_BYTES) return json(413, { error: "chunk too large" });
      const auth = await verifyRequest(request, "ika-share-v1", "PUT", url.pathname + url.search, body);
      if (auth.error) return auth.error;
      const { room } = await readRoom(env, code);
      if (!room) return json(404, { error: "room not found" });
      if (room.senderPubkey !== auth.pubkey) return json(403, { error: "not room sender" });
      if (room.state === "cancelled" || room.state === "failed") return json(410, { error: "transfer stopped" });

      if (idx >= 16) {
        const behind = await env.SKINS.head(chunkKey(code, idx - 16));
        if (behind) return json(200, { ok: false, wait: true });
      }
      await env.SKINS.put(chunkKey(code, idx), body);
      return json(200, { ok: true, wait: false });
    }

    if (method === "GET") {
      const auth = await verifyRequest(request, "ika-share-v1", "GET", url.pathname + url.search, new Uint8Array());
      if (auth.error) return auth.error;
      const obj = await env.SKINS.get(chunkKey(code, idx));
      if (!obj) return json(404, { error: "chunk not ready" });
      const bytes = await obj.arrayBuffer();
      await env.SKINS.delete(chunkKey(code, idx));
      return new Response(bytes, { status: 200, headers: { "content-type": "application/octet-stream" } });
    }

    return json(405, { error: "method not allowed" });
  }

  // Create room: POST /share/room
  if (url.pathname === "/share/room" && method === "POST") {
    const body = new Uint8Array(await request.arrayBuffer());
    if (body.length > MAX_ROOM_JSON_BYTES) return json(413, { error: "message too large" });
    const auth = await verifyRequest(request, "ika-share-v1", "POST", url.pathname + url.search, body);
    if (auth.error) return auth.error;

    let payload;
    try {
      payload = JSON.parse(new TextDecoder().decode(body));
    } catch {
      return json(400, { error: "bad json" });
    }
    if (!payload || !SHARE_ID_RE.test(payload.id || "")) return json(400, { error: "bad room payload" });

    let code = "";
    const preferred = String(payload.code || "").trim();
    if (ROOM_CODE_RE.test(preferred)) {
      const existing = await readRoom(env, preferred);
      if (!existing.room || existing.room.senderPubkey === auth.pubkey || ["done", "cancelled", "failed"].includes(existing.room.state)) {
        code = preferred;
      }
    }
    if (!code) {
      for (let i = 0; i < 12; i++) {
        const rand = new Uint32Array(1);
        crypto.getRandomValues(rand);
        const candidate = String(100000 + (rand[0] % 900000));
        const existing = await readRoom(env, candidate);
        if (!existing.room) {
          code = candidate;
          break;
        }
      }
    }
    if (!code) return json(503, { error: "no free codes, try again" });

    const now = Date.now();
    const senderName = String(payload.sender || "Jugador").trim().slice(0, 24);
    const recipientName = String(payload.recipient || "").trim().slice(0, 24);
    const room = {
      code,
      id: payload.id,
      senderPubkey: auth.pubkey,
      receiverPubkey: null,
      sender: senderName,
      recipient: recipientName || null,
      meta: payload.meta || {},
      offer: payload.offer || null,
      answer: null,
      mode: "webrtc",
      state: "waiting",
      error: null,
      created: now,
      updated: now,
    };
    await env.SKINS.put(roomKey(code), JSON.stringify(room));
    if (recipientName) {
      await env.SKINS.put(userRoomKey(recipientName), JSON.stringify({ code, updated: now }));
    } else if (senderName) {
      await env.SKINS.put(userRoomKey(senderName), JSON.stringify({ code, updated: now }));
    }
    return json(200, publicRoom(room));
  }

  // Lookup room by username: GET /share/room/by-user/{username}
  const byUserMatch = url.pathname.match(/^\/share\/room\/by-user\/([^/]+)$/);
  if (byUserMatch && method === "GET") {
    const auth = await verifyRequest(request, "ika-share-v1", "GET", url.pathname + url.search, new Uint8Array());
    if (auth.error) return auth.error;
    const rawUser = decodeURIComponent(byUserMatch[1]);
    const { room } = await resolveUserRoom(env, rawUser);
    if (!room) return json(404, { error: "room not found" });
    return json(200, publicRoom(room));
  }

  // Room state: GET / POST / DELETE /share/room/{code}
  const roomMatch = url.pathname.match(/^\/share\/room\/(\d{6})$/);
  if (!roomMatch) return json(404, { error: "not found" });
  const code = roomMatch[1];
  if (!ROOM_CODE_RE.test(code)) return json(400, { error: "invalid code" });
  if (method !== "GET" && method !== "POST" && method !== "DELETE") return json(405, { error: "method not allowed" });

  const body = method === "POST" ? new Uint8Array(await request.arrayBuffer()) : new Uint8Array();
  if (body.length > MAX_ROOM_JSON_BYTES) return json(413, { error: "message too large" });
  const auth = await verifyRequest(request, "ika-share-v1", method, url.pathname + url.search, body);
  if (auth.error) return auth.error;

  if (method === "GET") {
    const { room } = await readRoom(env, code);
    if (!room) return json(404, { error: "room not found" });
    return json(200, publicRoom(room));
  }

  if (method === "DELETE") {
    const { room } = await readRoom(env, code);
    if (room && (room.senderPubkey === auth.pubkey || room.receiverPubkey === auth.pubkey)) {
      await env.SKINS.delete(roomKey(code));
      if (room.recipient) await env.SKINS.delete(userRoomKey(room.recipient));
      if (room.sender) await env.SKINS.delete(userRoomKey(room.sender));
    }
    return json(200, { ok: true });
  }

  // POST /share/room/{code}
  let patch;
  try {
    patch = JSON.parse(new TextDecoder().decode(body));
  } catch {
    return json(400, { error: "bad json" });
  }
  for (let attempt = 0; attempt < 8; attempt++) {
    if (attempt) await new Promise((resolve) => setTimeout(resolve, 20 + Math.random() * 50 * attempt));
    const { room, etag } = await readRoom(env, code);
    if (!room) return json(404, { error: "room not found" });

    if (patch.answer && (!room.receiverPubkey || room.receiverPubkey === auth.pubkey)) {
      room.receiverPubkey = auth.pubkey;
      room.answer = String(patch.answer);
      if (room.state === "waiting") room.state = "connecting";
    }
    const isParticipant = auth.pubkey === room.senderPubkey || auth.pubkey === room.receiverPubkey || !room.receiverPubkey;
    if (isParticipant) {
      if (!room.receiverPubkey && auth.pubkey !== room.senderPubkey) {
        room.receiverPubkey = auth.pubkey;
      }
      if (patch.mode === "relay") {
        room.mode = "relay";
        if (room.state === "waiting" || room.state === "connecting") room.state = "transferring";
      }
      if (["connecting", "transferring", "done", "cancelled", "failed"].includes(patch.state)) {
        room.state = patch.state;
      }
      if (patch.error) {
        room.error = String(patch.error).slice(0, 200);
      }
    } else {
      return json(403, { error: "room already in use" });
    }
    room.updated = Date.now();
    const written = await env.SKINS.put(roomKey(code), JSON.stringify(room), etag ? { onlyIf: { etagMatches: etag } } : undefined);
    if (written) return json(200, publicRoom(room));
  }
  return json(503, { error: "busy, try again" });
}

async function handleShare(request, env, url) {
  if (url.pathname === "/share/room" || url.pathname.startsWith("/share/room/")) {
    return handleRoom(request, env, url);
  }
  const m = url.pathname.match(/^\/share\/box\/([^/]+)$/);
  if (!m) return json(404, { error: "not found" });
  const rawName = decodeURIComponent(m[1]);
  if (!NAME_RE.test(rawName)) return json(400, { error: "invalid name" });
  const name = rawName.toLowerCase();
  const method = request.method;
  if (method !== "GET" && method !== "POST" && method !== "DELETE") return json(405, { error: "method not allowed" });

  const body = method === "POST" ? new Uint8Array(await request.arrayBuffer()) : new Uint8Array();
  if (body.length > MAX_MESSAGE_BYTES) return json(413, { error: "message too large" });
  const auth = await verifyRequest(request, "ika-share-v1", method, url.pathname + url.search, body);
  if (auth.error) return auth.error;

  if (method === "GET") {
    const denied = await requireOwner(env, name, auth.pubkey);
    if (denied) return denied;
    const { messages } = await readBox(env, name);
    const now = Date.now();
    return json(200, { messages: messages.filter((msg) => alive(msg, now)) });
  }

  if (method === "DELETE") {
    const denied = await requireOwner(env, name, auth.pubkey);
    if (denied) return denied;
    const keys = new Set((url.searchParams.get("keys") || "").split(",").filter(Boolean));
    if (!(await updateBox(env, name, (messages) => messages.filter((msg) => !keys.has(msg.key))))) return json(503, { error: "busy, try again" });
    return json(200, { ok: true });
  }

  // POST: a message from `from` (the signer) to `name`
  const rawFrom = url.searchParams.get("from") || "";
  if (!NAME_RE.test(rawFrom)) return json(400, { error: "invalid sender name" });
  const from = rawFrom.toLowerCase();
  const denied = await requireOwner(env, from, auth.pubkey);
  if (denied) return denied;
  if (!(await ownerOf(env, name))) return json(404, { error: "unknown recipient" });

  let message;
  try {
    message = JSON.parse(new TextDecoder().decode(body));
  } catch {
    return json(400, { error: "bad json" });
  }
  if (!message || !SHARE_ID_RE.test(message.id || "") || !SHARE_TTL_MS[message.type]) return json(400, { error: "bad message" });
  const entry = { key: crypto.randomUUID().replace(/-/g, "").slice(0, 20), id: message.id, from, type: message.type, data: message.data ?? null, ts: Date.now() };
  const same = (msg) => msg.id === entry.id && msg.from === entry.from && msg.type === entry.type;
  if (!(await updateBox(env, name, (messages) => [...messages.filter((msg) => !same(msg)), entry]))) return json(503, { error: "busy, try again" });
  return json(200, { ok: true });
}

async function handleSkins(request, env, url) {
  const m = url.pathname.match(/^\/skins\/([^/]+?)(\.png)?$/);
  if (!m) return json(404, { error: "not found" });
  const rawName = decodeURIComponent(m[1]);
  if (!NAME_RE.test(rawName)) return json(400, { error: "invalid name" });
  const name = rawName.toLowerCase(); // Minecraft names are case-insensitive
  const objectKey = skinKey(name);

  if (request.method === "GET" || request.method === "HEAD") {
    const obj = await env.SKINS.get(objectKey);
    if (!obj) return json(404, { error: "no skin for this name" });
    return new Response(request.method === "HEAD" ? null : obj.body, {
      headers: {
        "content-type": "image/png",
        "cache-control": "public, max-age=60",
        etag: obj.httpEtag,
      },
    });
  }

  if (request.method === "PUT" || request.method === "DELETE") {
    const body = request.method === "PUT" ? new Uint8Array(await request.arrayBuffer()) : new Uint8Array();
    if (body.length > MAX_PNG_BYTES) return json(413, { error: "skin too large" });
    const auth = await verifyRequest(request, "ika-skin-v1", request.method, name, body);
    if (auth.error) return auth.error;

    const owner = await ownerOf(env, name);
    if (owner && owner !== auth.pubkey) {
      return json(403, { error: "name owned by another device" });
    }

    if (request.method === "DELETE") {
      // releases the name: its skin, its claim and its mailbox
      const existing = await env.SKINS.head(objectKey);
      await env.SKINS.delete([objectKey, nameKey(name), boxKey(name)]);
      return json(200, { ok: true, deleted: !!existing });
    }

    const problem = checkSkinPng(body);
    if (problem) return json(400, { error: problem });
    await env.SKINS.put(objectKey, body, {
      httpMetadata: { contentType: "image/png" },
      customMetadata: { pubkey: auth.pubkey, updated: String(Date.now()) },
    });
    return json(200, { ok: true, claimed: !owner });
  }

  return json(405, { error: "method not allowed" });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/" && request.method === "GET") {
      return json(200, { service: "ika-skins", version: 3 });
    }
    if (url.pathname.startsWith("/share/")) return handleShare(request, env, url);
    return handleSkins(request, env, url);
  },
};
