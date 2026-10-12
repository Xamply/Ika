// Gestor de Instancia: the Minecraft island under the plates (three.js r128), drawn by a worker on an
// OffscreenCanvas. Building it and, above all, compiling its ~30 shader programs for the first frame
// takes seconds after a cold start: here that never blocks the launcher's UI thread.
// three.js r128 + its classic examples come from /vendor/three-r128 (the design was tuned with r128's
// lighting and colour pipeline, newer versions change it).
// In:  { type: "init", canvas, consts: { BASE_Y, HALF_W, HALF_H }, layout } | { type: "layout", layout } | { type: "draw", t }
// Out: { type: "ready" } once the first frame is drawn | { type: "failed", error }

const THREE_FILES = [
  "three.min.js",
  "CopyShader.js",
  "LuminosityHighPassShader.js",
  "EffectComposer.js",
  "RenderPass.js",
  "ShaderPass.js",
  "SavePass.js",
  "UnrealBloomPass.js",
];

// Same values as manager-plates.js (sent with "init"): the island is aligned with the plates.
let BASE_Y = 0, HALF_W = 0, HALF_H = 0;
// Canvas size and where the plates' square sits in it, in CSS pixels: the worker can't measure the DOM.
let layout = { width: 0, height: 0, stageLeft: 0, stageTop: 0, stageWidth: 0, dpr: 1 };

function rng(a) {
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}


/* =========================================================
   MUNDO DE MINECRAFT bajo las placas — estilo "Complementary":
   iluminación suave por vértice (AO), sombras, cielo azul en las sombras, bruma, agua con olas,
   bloom y tonemapping cinematográfico. Cámara ortográfica alineada con la perspectiva de las placas.
   ========================================================= */
function buildWorld(T, cv) {
  let renderer;
  try {
    renderer = new T.WebGLRenderer({ canvas: cv, antialias: true, alpha: true, premultipliedAlpha: true, powerPreference: "high-performance" });
  } catch (e) {
    return null;
  }
  renderer.setClearColor(0x000000, 0);
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = T.PCFShadowMap;
  // la isla no se mueve: las sombras se calculan una vez (y al cambiar de tamaño), no en cada frame
  renderer.shadowMap.autoUpdate = false;
  renderer.shadowMap.needsUpdate = true;
  const lin = (hex) => new T.Color(hex).convertSRGBToLinear();

  const scene = new T.Scene();
  const N = 34, Y0 = BASE_Y, ISL = HALF_W, K = ISL / (N * Math.SQRT1_2), SEA = 1; // K = píxeles por bloque (en el lienzo de 1000)
  const EL = Math.asin(HALF_H / HALF_W), AZ = Math.PI / 4;
  const cam = new T.OrthographicCamera(-30, 30, 30, -30, 1, 700);
  cam.position.set(Math.cos(EL) * Math.sin(AZ) * 300, Math.sin(EL) * 300 + SEA, Math.cos(EL) * Math.cos(AZ) * 300);
  cam.lookAt(0, SEA, 0);
  scene.fog = new T.Fog(new T.Color(0.03, 0.06, 0.17), 292, 345); // bruma con la profundidad

  /* ---------- ruido ---------- */
  const r = rng(4242);
  const h1 = (n) => {
    n = Math.sin(n * 127.1 + 311.7) * 43758.5453;
    return n - Math.floor(n);
  };
  const h2 = (x, y) => h1(x * 57.3 + y * 131.9);
  const vn = (x, y) => {
    const xi = Math.floor(x), yi = Math.floor(y), xf = x - xi, yf = y - yi, u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
    const a = h2(xi, yi), b = h2(xi + 1, yi), c = h2(xi, yi + 1), d = h2(xi + 1, yi + 1);
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
  };
  const fbm = (x, y) => vn(x, y) * 0.5 + vn(x * 2.1, y * 2.1) * 0.3 + vn(x * 4.3, y * 4.3) * 0.2;

  /* ---------- texturas 16×16 ---------- */
  function tex(draw, seed) {
    const c = new OffscreenCanvas(1, 1);
    c.width = c.height = 16;
    const g = c.getContext("2d");
    const img = g.createImageData(16, 16);
    const rr = rng(seed);
    for (let y = 0; y < 16; y++)
      for (let x = 0; x < 16; x++) {
        const p = draw(x, y, rr);
        const o = (y * 16 + x) * 4;
        img.data[o] = p[0];
        img.data[o + 1] = p[1];
        img.data[o + 2] = p[2];
        img.data[o + 3] = p[3] === undefined ? 255 : p[3];
      }
    g.putImageData(img, 0, 0);
    const t = new T.CanvasTexture(c);
    t.magFilter = T.NearestFilter;
    t.minFilter = T.NearestFilter;
    t.generateMipmaps = false;
    t.encoding = T.sRGBEncoding;
    return t;
  }
  const J = (c, rr, a) => {
    const f = 1 + (rr() - 0.5) * a;
    return [c[0] * f, c[1] * f, c[2] * f];
  };
  const DI = [128, 88, 58];
  const TX = {
    grassTop: tex((x, y, rr) => J(rr() < 0.18 ? [78, 132, 44] : [94, 152, 54], rr, 0.2), 1),
    grassSide: tex((x, y, rr) => {
      const e = 2 + ((h2(x, 7) * 3) | 0);
      return y < e ? J([100, 160, 58], rr, 0.18) : J(rr() < 0.14 ? [112, 66, 34] : DI, rr, 0.22);
    }, 2),
    dirt: tex((x, y, rr) => J(rr() < 0.14 ? [112, 66, 34] : DI, rr, 0.22), 3),
    stone: tex((x, y, rr) => {
      const v = vn(x * 0.45, y * 0.45);
      return J(v > 0.62 ? [98, 98, 104] : v < 0.3 ? [142, 142, 148] : [124, 124, 130], rr, 0.1);
    }, 4),
    sand: tex((x, y, rr) => J([186, 170, 128], rr, 0.1), 5),
    gravel: tex((x, y, rr) => J(rr() < 0.3 ? [100, 94, 92] : [134, 126, 122], rr, 0.2), 6),
    path: tex((x, y, rr) => (y < 2 ? J([148, 122, 66], rr, 0.12) : J([170, 140, 80], rr, 0.14)), 7),
    farm: tex((x, y, rr) => J(y % 4 === 0 ? [72, 44, 24] : [98, 62, 36], rr, 0.15), 8),
    logSide: tex((x, y, rr) => J(x % 4 === 1 || rr() < 0.08 ? [76, 58, 34] : [106, 84, 52], rr, 0.12), 9),
    logTop: tex((x, y, rr) => {
      const d = Math.max(Math.abs(x - 7.5), Math.abs(y - 7.5));
      return J(d > 6.5 ? [106, 84, 52] : (d | 0) % 2 ? [176, 142, 90] : [156, 122, 74], rr, 0.06);
    }, 10),
    birchSide: tex((x, y, rr) => J(rr() < 0.12 ? [50, 46, 40] : [220, 218, 206], rr, 0.06), 11),
    leaves: tex((x, y, rr) => {
      const v = vn(x * 0.45 + 2, y * 0.45 + 5);
      return J(v < 0.35 ? [34, 78, 28] : v < 0.65 ? [40, 90, 32] : [48, 102, 38], rr, 0.06);
    }, 12),
    birchLeaves: tex((x, y, rr) => {
      const v = vn(x * 0.45 + 7, y * 0.45 + 1);
      return J(v < 0.35 ? [66, 96, 38] : v < 0.65 ? [76, 108, 44] : [86, 120, 50], rr, 0.06);
    }, 13),
    planks: tex((x, y, rr) => J(y % 4 === 3 ? [120, 94, 56] : x === ((y >> 2) * 5 + 3) % 16 ? [134, 104, 62] : [170, 136, 84], rr, 0.08), 14),
    roof: tex((x, y, rr) => J(y % 4 === 3 ? [60, 38, 22] : x === ((y >> 2) * 6 + 2) % 16 ? [72, 46, 28] : [98, 66, 40], rr, 0.1), 27),
    roofTop: tex((x, y, rr) => {
      const v = y % 4 === 3 ? 205 : x === ((y >> 2) * 6 + 2) % 16 ? 225 : 255;
      const f = 1 - rr() * 0.06;
      return [v * f, v * f, v * f];
    }, 31),
    spruceLog: tex((x, y, rr) => J(x % 5 === 2 || rr() < 0.08 ? [46, 32, 20] : [66, 47, 30], rr, 0.12), 28),
    plaster: tex((x, y, rr) => J(rr() < 0.08 ? [206, 196, 176] : [226, 218, 200], rr, 0.05), 29),
    spruceDoor: tex((x, y, rr) => (x === 0 || x === 15 || y === 0 || y === 15 || y === 8 ? [44, 30, 18] : y > 2 && y < 7 && x > 3 && x < 12 ? [255, 214, 140] : J([88, 62, 38], rr, 0.1)), 30),
    darkPlanks: tex((x, y, rr) => J(y % 4 === 3 ? [52, 34, 20] : [78, 52, 30], rr, 0.1), 15),
    cobble: tex((x, y, rr) => {
      const v = vn(x * 0.7 + 3, y * 0.7);
      return J(v > 0.52 && v < 0.6 ? [70, 70, 74] : v > 0.6 ? [150, 150, 156] : [118, 118, 124], rr, 0.12);
    }, 16),
    brick: tex((x, y, rr) => {
      const mortar = y % 4 === 3 || ((y >> 2) % 2 ? x + 4 : x) % 8 === 7;
      return mortar ? [170, 160, 150] : J([150, 74, 58], rr, 0.15);
    }, 17),
    glow: tex((x, y, rr) => J(rr() < 0.35 ? [255, 236, 170] : [236, 166, 74], rr, 0.15), 18),
    glass: tex((x, y) => (x === 0 || y === 0 || x === 15 || y === 15 ? [214, 236, 240, 255] : x - y === 4 || x - y === 6 ? [235, 250, 255, 170] : [170, 215, 235, 50]), 19),
    door: tex((x, y, rr) => (x === 0 || x === 15 || y === 0 || y === 15 || y === 7 ? [70, 48, 28] : y > 1 && y < 6 && x > 3 && x < 12 ? [150, 200, 220, 255] : J([116, 84, 48], rr, 0.1)), 20),
    hay: tex((x, y, rr) => (y === 3 || y === 12 ? [150, 60, 40] : J([200, 166, 56], rr, 0.18)), 21),
    tallgrass: tex((x, y, rr) => (h2(x, 3) > 0.45 && y > 3 + h2(x, 9) * 6 ? J([86, 150, 50], rr, 0.2) : [0, 0, 0, 0]), 22),
    wheat: tex((x, y, rr) => (x % 3 !== 1 && y > 2 ? J(y < 7 ? [210, 180, 70] : [150, 160, 60], rr, 0.15) : [0, 0, 0, 0]), 23),
    poppy: tex((x, y) => (x >= 6 && x <= 9 && y >= 3 && y <= 6 ? [210, 40, 36] : (x === 7 || x === 8) && y > 6 ? [60, 120, 40] : [0, 0, 0, 0]), 24),
    dandelion: tex((x, y) => (x >= 6 && x <= 9 && y >= 4 && y <= 6 ? [250, 214, 40] : x === 7 && y > 6 ? [60, 120, 40] : [0, 0, 0, 0]), 25),
    lantern: tex((x, y) => (x < 3 || x > 12 ? [0, 0, 0, 0] : y < 3 || y > 13 || x === 3 || x === 12 ? [50, 50, 56] : [255, 190, 90]), 26),
  };
  // tipo de bloque → textura por cara [lado, arriba, abajo]
  const BT = {
    grass: ["grassSide", "grassTop", "dirt"], dirt: ["dirt"], stone: ["stone"], sand: ["sand"], gravel: ["gravel"], path: ["path", "path", "dirt"],
    farm: ["dirt", "farm", "dirt"], log: ["logSide", "logTop", "logTop"], birch: ["birchSide", "logTop", "logTop"],
    leaves: ["leaves"], birchLeaves: ["birchLeaves"], planks: ["planks"], dark: ["darkPlanks"], roof: ["roof"], spruceLog: ["spruceLog", "logTop", "logTop"], plaster: ["plaster"], sdoor: ["spruceDoor", "roof", "roof"], cobble: ["cobble"], brick: ["brick"],
    glow: ["glow"], glass: ["glass"], door: ["door", "darkPlanks", "darkPlanks"], hay: ["hay", "hay", "hay"],
  };
  const CUTOUT = new Set(["glass"]);

  /* ---------- mundo ---------- */
  const W3 = new Map(), K3 = (x, y, z) => x + "," + y + "," + z;
  const set = (x, y, z, t) => {
    if (t) W3.set(K3(x, y, z), t);
    else W3.delete(K3(x, y, z));
  };
  const get = (x, y, z) => W3.get(K3(x, y, z));
  const top = {}, isLand = (i, j) => top[i + "," + j] !== undefined, half = N / 2;
  const water = new Set(); // bloques de agua de río/estanque (superficie)
  for (let i = 0; i < N; i++)
    for (let j = 0; j < N; j++) {
      const ci = ((i + 0.5) / N) * 2 - 1, cj = ((j + 0.5) / N) * 2 - 1;
      const d = Math.max(Math.abs(ci), Math.abs(cj)); // mismo rombo que las placas
      if (d > 0.94 && fbm(i * 0.3 + 3, j * 0.3 + 7) < 0.42) continue;
      const back = Math.max(0, Math.min(1, -(ci + cj) / 2 + 0.5)); // 1 = fondo, 0 = frente
      // terrazas escalonadas, más altas al fondo
      let t = 7 + 2 * Math.floor(fbm(i * 0.11 + 1, j * 0.11 + 2) * 3.2) + Math.floor(back * back * 7 * fbm(i * 0.1 + 9, j * 0.1 + 4));
      if (d > 0.9) t -= 2 + (h2(i, j) > 0.5 ? 2 : 0);
      t = Math.max(4, Math.min(19, t));
      top[i + "," + j] = t;
    }
  // zona del pueblo (frente): terreno plano
  const village = (i, j) => {
    const ci = ((i + 0.5) / N) * 2 - 1, cj = ((j + 0.5) / N) * 2 - 1;
    return ci + cj > 0.35 && Math.abs(ci - cj) < 1.05;
  };
  Object.keys(top).forEach((k) => {
    const [i, j] = k.split(",").map(Number);
    if (village(i, j) && top[k] > 6) top[k] = Math.max(7, Math.min(9, top[k]));
  });
  // río que baja de la montaña hacia un lado
  const riv = [];
  for (let i = 0; i < N; i++)
    for (let j = 0; j < N; j++) {
      if (!isLand(i, j) || village(i, j)) continue;
      if (Math.abs(fbm(i * 0.07 + 30, j * 0.07 + 12) - 0.5) < 0.028) {
        riv.push([i, j]);
        top[i + "," + j] = Math.max(4, top[i + "," + j] - 2);
      }
    }
  // columnas
  Object.keys(top).forEach((k) => {
    const [i, j] = k.split(",").map(Number), t = top[k];
    for (let y = SEA - 2; y < t; y++) {
      let ty;
      if (y === t - 1) ty = t >= 17 ? "stone" : "grass";
      else if (y >= t - 7 || h2(i * y, j) > 0.75) ty = "dirt";
      else ty = "stone";
      set(i, y, j, ty);
    }
  });
  riv.forEach(([i, j]) => {
    const t = top[i + "," + j];
    set(i, t - 1, j, "sand");
    set(i, t, j, null);
    water.add(i + "," + (t - 1) + "," + j);
  });

  /* ---------- estructuras de frente ---------- */
  const lamps = [];
  function clear(x0, z0, w, d, h) {
    for (let a = 0; a < w; a++) for (let b = 0; b < d; b++) for (let y = h; y < h + 14; y++) set(x0 + a, y, z0 + b, null);
  }
  function house(x0, z0, w, d, h, face) {
    // casa de entramado: base de piedra, vigas de abeto, muros de yeso, frontón y puerta mirando a la cámara
    clear(x0 - 1, z0 - 1, w + 2, d + 2, h);
    for (let a = -1; a <= w; a++) for (let b = -1; b <= d; b++) if (top[x0 + a + "," + (z0 + b)] === h) set(x0 + a, h - 1, z0 + b, "dirt");
    for (let a = 0; a < w; a++) for (let b = 0; b < d; b++) set(x0 + a, h - 1, z0 + b, "cobble");
    const HH = 5, fx = face === "x";
    const L1 = fx ? d : w; // largo de la fachada frontal
    const mid = Math.floor(L1 / 2);
    for (let a = 0; a < w; a++)
      for (let b = 0; b < d; b++)
        for (let y = h; y < h + HH; y++) {
          const eA = a === 0 || a === w - 1, eB = b === 0 || b === d - 1;
          if (!eA && !eB) continue;
          const front = fx ? a === w - 1 : b === d - 1, side = fx ? b === d - 1 : a === w - 1;
          const along = front ? (fx ? b : a) : fx ? a : b, len = front ? L1 : fx ? w : d;
          const corner = eA && eB, pillar = corner || along % 3 === 0 || along === len - 1;
          let t;
          if (y === h) t = "cobble";
          else if (pillar || y === h + HH - 1) t = "spruceLog";
          else t = "plaster";
          if ((front || side) && !pillar && y === h + 2 && !(front && along === mid)) t = "glass";
          if (front && along === mid && y < h + 2) t = "sdoor";
          if (front && along === mid && y === h + 2) t = "plaster";
          set(x0 + a, y, z0 + b, t);
        }
    // techo de abeto a dos aguas con alero; el triángulo del frontón da a la cámara
    const span = fx ? d : w, peak = Math.ceil(span / 2), top0 = h + HH;
    for (let s = 0; s <= peak; s++) {
      for (let a = -1; a <= w; a++)
        for (let b = -1; b <= d; b++) {
          const along = fx ? b : a, across = fx ? a : b, alen = fx ? w : d;
          if (along < s - 1 || along > span - s) continue;
          const slope = along === s - 1 || along === span - s || s === peak;
          if (slope) set(x0 + a, top0 + s, z0 + b, "roof");
          else if (across === -1 || across === alen) set(x0 + a, top0 + s, z0 + b, null);
          else if (across === 0 || across === alen - 1) set(x0 + a, top0 + s, z0 + b, along === mid ? "glass" : "plaster");
        }
    }
    // viga de cumbrera y chimenea de piedra
    for (let k = -1; k <= (fx ? w : d); k++) set(fx ? x0 + k : x0 + Math.floor(w / 2), top0 + peak, fx ? z0 + Math.floor(d / 2) : z0 + k, "roof");
    for (let y = top0; y < top0 + peak + 3; y++) set(x0 + 1, y, z0 + 1, "cobble");
  }
  function cabin(x0, z0, w, d, h, face) {
    // cabaña de troncos de roble, techo a cuatro aguas de roble oscuro, porche con columnas
    clear(x0 - 1, z0 - 1, w + 3, d + 3, h);
    for (let a = -1; a <= w + 1; a++) for (let b = -1; b <= d + 1; b++) if (top[x0 + a + "," + (z0 + b)] === h) set(x0 + a, h - 1, z0 + b, "dirt");
    for (let a = 0; a < w; a++) for (let b = 0; b < d; b++) set(x0 + a, h - 1, z0 + b, "stone");
    const HH = 4, fx = face === "x", L1 = fx ? d : w, mid = Math.floor(L1 / 2);
    for (let a = 0; a < w; a++)
      for (let b = 0; b < d; b++)
        for (let y = h; y < h + HH; y++) {
          const eA = a === 0 || a === w - 1, eB = b === 0 || b === d - 1;
          if (!eA && !eB) continue;
          const front = fx ? a === w - 1 : b === d - 1, side = fx ? b === d - 1 : a === w - 1, along = front ? (fx ? b : a) : fx ? a : b;
          let t = eA && eB ? "birch" : "log";
          if ((front || side) && !(eA && eB) && y === h + 1 && along !== mid && along % 2 === 0) t = "glass";
          if (front && along === mid && y < h + 2) t = "door";
          set(x0 + a, y, z0 + b, t);
        }
    // porche: piso y dos columnas al frente
    for (let k = 0; k < L1; k++) {
      const px = fx ? x0 + w : x0 + k, pz = fx ? z0 + k : z0 + d;
      if (top[px + "," + pz] === h) set(px, h - 1, pz, "dirt");
    }
    [mid - 1, mid + 1].forEach((k) => {
      const px = fx ? x0 + w : x0 + k, pz = fx ? z0 + k : z0 + d;
      set(px, h, pz, "birch");
      set(px, h + 1, pz, "birch");
      set(px, h + 2, pz, "birch");
    });
    // techo a cuatro aguas (pirámide escalonada) que también cubre el porche
    const ex = fx ? 1 : 0, ez = fx ? 0 : 1;
    for (let s = 0; ; s++) {
      const a0 = -1 + s, a1 = w + ex - s, b0 = -1 + s, b1 = d + ez - s;
      if (a0 > a1 || b0 > b1) break;
      for (let a = a0; a <= a1; a++)
        for (let b = b0; b <= b1; b++) {
          if (a === a0 || a === a1 || b === b0 || b === b1) set(x0 + a, h + HH + s - 1, z0 + b, "dark");
        }
    }
    // farol colgado junto a la puerta y chimenea de ladrillo
    const lx = fx ? x0 + w : x0 + mid + 2, lz = fx ? z0 + mid + 2 : z0 + d;
    if (isLand(lx, lz) && !get(lx, h + 2, lz)) {
      set(lx, h + 2, lz, "glow");
      lamps.push([lx, h + 2, lz]);
    }
    for (let y = h; y < h + HH + 4; y++) set(x0, y, z0, "brick");
  }
  const at = (fi, fj) => [Math.round(fi * N), Math.round(fj * N)];
  // casas mirando a la cámara, distribuidas por la franja frontal
  [[0.47, 0.79, 7, 6, "x", house], [0.74, 0.44, 6, 6, "z", cabin]].forEach(([fi, fj, w, d, f, fn]) => {
    const [x, z] = at(fi, fj);
    if (isLand(x, z) && isLand(x + w, z + d)) fn(x, z, w, d, top[x + "," + z], f);
  });
  // torre de vigía de piedra con faro en la punta
  (function () {
    const [x0, z0] = at(0.62, 0.62);
    if (!isLand(x0, z0)) return;
    const h = top[x0 + "," + z0];
    clear(x0 - 1, z0 - 1, 5, 5, h);
    for (let y = h; y < h + 9; y++)
      for (let a = 0; a < 3; a++)
        for (let b = 0; b < 3; b++) {
          const edge = a !== 1 || b !== 1;
          if (!edge) continue;
          let t = (a === 1 || b === 1) && y % 3 === 1 && y > h + 1 ? "glass" : "cobble";
          if (a === 2 && b === 1 && y < h + 2) t = "door";
          set(x0 + a, y, z0 + b, t);
        }
    for (let a = -1; a < 4; a++)
      for (let b = -1; b < 4; b++) {
        set(x0 + a, h + 9, z0 + b, "dark");
        if ((a === -1 || a === 3 || b === -1 || b === 3) && (a + b) % 2 === 0) set(x0 + a, h + 10, z0 + b, "cobble");
      }
    set(x0 + 1, h + 10, z0 + 1, "glow");
    lamps.push([x0 + 1, h + 10, z0 + 1]);
  })();
  // plaza con fogata centrada (frente al pueblo)
  let campfire = null;
  (function () {
    const c0 = Math.round(0.79 * N), cx = c0, cz = c0;
    if (!isLand(cx, cz)) return;
    const h = top[cx + "," + cz];
    for (let a = -3; a <= 3; a++)
      for (let b = -3; b <= 3; b++) {
        const x = cx + a, z = cz + b;
        if (!isLand(x, z)) continue;
        if (Math.abs(a) + Math.abs(b) > 4) continue;
        for (let y = h; y < h + 6; y++) if (get(x, y, z) !== "glow" && get(x, y, z) !== "log") set(x, y, z, null);
        for (let y = top[x + "," + z] - 1; y < h - 1; y++) set(x, y, z, "dirt");
        top[x + "," + z] = h;
        set(x, h - 1, z, Math.abs(a) + Math.abs(b) <= 2 ? "cobble" : "dirt");
      }
    campfire = [cx, h, cz];
  })();
  // pozo
  (function () {
    const [x0, z0] = at(0.84, 0.7);
    if (!isLand(x0, z0) || !isLand(x0 + 2, z0 + 2)) return;
    const h = top[x0 + "," + z0];
    for (let a = 0; a < 3; a++)
      for (let b = 0; b < 3; b++) {
        set(x0 + a, h, z0 + b, a === 1 && b === 1 ? null : "cobble");
        if (a === 1 && b === 1) water.add(x0 + a + "," + h + "," + (z0 + b));
      }
    [[0, 0], [2, 0], [0, 2], [2, 2]].forEach(([a, b]) => {
      set(x0 + a, h + 1, z0 + b, "log");
      set(x0 + a, h + 2, z0 + b, "log");
    });
    for (let a = 0; a < 3; a++) for (let b = 0; b < 3; b++) set(x0 + a, h + 3, z0 + b, "dark");
  })();

  // árboles (no en el pueblo)
  function oak(i, j, t, birch) {
    const th = birch ? 5 + ((r() * 2) | 0) : 4 + ((r() * 2) | 0), Lf = birch ? "birchLeaves" : "leaves";
    for (let y = t; y < t + th; y++) set(i, y, j, birch ? "birch" : "log");
    for (let dy = -2; dy <= 1; dy++) {
      const rad = dy < 0 ? 2 : 1;
      for (let a = -rad; a <= rad; a++)
        for (let b = -rad; b <= rad; b++) {
          if (Math.abs(a) === rad && Math.abs(b) === rad && (dy >= 0 || r() < 0.5)) continue;
          if (!get(i + a, t + th + dy, j + b)) set(i + a, t + th + dy, j + b, Lf);
        }
    }
  }
  const trees = [];
  for (let n = 0; n < 3000 && trees.length < 30; n++) {
    const i = 1 + ((r() * (N - 2)) | 0), j = 1 + ((r() * (N - 2)) | 0), t = top[i + "," + j];
    if (!t || t < 3 || village(i, j) || Math.abs(i - j) > N * 0.56 || get(i, t - 1, j) !== "grass" || get(i, t, j) || trees.some(([a, b]) => Math.abs(a - i) + Math.abs(b - j) < 4)) continue;
    trees.push([i, j]);
    oak(i, j, t, r() < 0.3);
  }

  // faroles al pie de algunos árboles: su luz cálida ilumina las copas
  trees.forEach(([ti, tj], n) => {
    if (n % 2) return;
    for (const [di, dj] of [[2, 0], [0, 2], [-2, 0], [0, -2], [2, 1], [1, 2]]) {
      const i = ti + di, j = tj + dj, t = top[i + "," + j];
      if (!t || village(i, j) || get(i, t - 1, j) !== "grass" || get(i, t, j) || get(i, t + 1, j) || get(i, t + 2, j)) continue;
      set(i, t, j, "log");
      set(i, t + 1, j, "glow");
      lamps.push([i, t + 1, j]);
      break;
    }
  });
  // faroles sueltos por la isla (focos de luz cálida)
  for (let n = 0; n < 800 && lamps.length < 16; n++) {
    const i = 1 + ((r() * (N - 2)) | 0), j = 1 + ((r() * (N - 2)) | 0), t = top[i + "," + j];
    if (!t || village(i, j) || get(i, t - 1, j) !== "grass" || get(i, t, j) || get(i, t + 1, j) || lamps.some(([a, , b]) => Math.abs(a - i) + Math.abs(b - j) < 5)) continue;
    set(i, t, j, "log");
    set(i, t + 1, j, "glow");
    lamps.push([i, t + 1, j]);
  }

  /* ---------- geometría con luz suave por vértice (AO) ---------- */
  const OPQ = (t) => t && !CUTOUT.has(t);
  const FACES = [
    { n: [1, 0, 0], c: [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]], k: 0, sh: 0.72 },
    { n: [-1, 0, 0], c: [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]], k: 0, sh: 0.62 },
    { n: [0, 1, 0], c: [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]], k: 1, sh: 1 },
    { n: [0, -1, 0], c: [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], k: 2, sh: 0.5 },
    { n: [0, 0, 1], c: [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]], k: 0, sh: 0.72 },
    { n: [0, 0, -1], c: [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]], k: 0, sh: 0.6 },
  ];
  const LIGHTS = lamps.map(([x, y, z]) => [x + 0.5, y, z + 0.5, 6]);
  if (campfire) LIGHTS.push([campfire[0] + 0.5, campfire[1], campfire[2] + 0.5, 7]);
  const AOL = [0.38, 0.6, 0.8, 1], AOLEAF = [0.62, 0.76, 0.9, 1];
  const Bf = {}; // por textura
  const buf = (n) => Bf[n] || (Bf[n] = { p: [], nn: [], uv: [], c: [], i: [] });
  const occ = (x, y, z) => (OPQ(get(x, y, z)) ? 1 : 0), occL = (x, y, z) => (get(x, y, z) ? 1 : 0);
  W3.forEach((t, k) => {
    const [x, y, z] = k.split(",").map(Number), def = BT[t];
    if (!def) return;
    const cut = CUTOUT.has(t);
    FACES.forEach((Fc) => {
      const nx = x + Fc.n[0], ny = y + Fc.n[1], nz = z + Fc.n[2], nb = get(nx, ny, nz);
      if (cut) {
        if (nb === t) return;
      } else if (OPQ(nb)) return;
      if (Fc.n[1] === -1 && y <= SEA - 2) return;
      let tn = def[Math.min(Fc.k, def.length - 1)];
      if (tn === "roof" || tn === "darkPlanks") tn = "roofTop"; // todo el techo en #623426, las caras laterales más oscuras por el sombreado
      const b = buf(tn), base = b.p.length / 3;
      const ax = Fc.n.map(Math.abs), t1 = ax[0] ? [0, 1, 0] : [1, 0, 0], t2 = ax[2] ? [0, 1, 0] : [0, 0, 1];
      const aos = [];
      Fc.c.forEach((c, vi) => {
        const s1 = c[t1.indexOf(1)] ? 1 : -1, s2 = c[t2.indexOf(1)] ? 1 : -1;
        const O = cut && t !== "glass" ? occL : occ;
        const o1 = O(nx + t1[0] * s1, ny + t1[1] * s1, nz + t1[2] * s1), o2 = O(nx + t2[0] * s2, ny + t2[1] * s2, nz + t2[2] * s2);
        const oc = O(nx + t1[0] * s1 + t2[0] * s2, ny + t1[1] * s1 + t2[1] * s2, nz + t1[2] * s1 + t2[2] * s2);
        const ao = t === "glass" ? 3 : o1 && o2 ? 0 : 3 - (o1 + o2 + oc);
        aos.push(ao);
        b.p.push(x - half + c[0], y + c[1], z - half + c[2]);
        b.nn.push(...Fc.n);
        b.uv.push([0, 1, 1, 0][vi], [0, 0, 1, 1][vi]);
        // tinte: el césped varía un poco de tono; lo hondo se oscurece
        let tint = 1;
        if (tn === "grassTop") tint = 0.9 + fbm(x * 0.15, z * 0.15) * 0.22;
        if (tn === "leaves" || tn === "birchLeaves") tint = (0.92 + h2(x * 3 + y, z * 5 - y) * 0.12) * (0.85 + 0.15 * Math.min(1, (y - (top[x + "," + z] || 0)) / 5));
        const deep = Math.max(0.55, Math.min(1, 1 - ((top[x + "," + z] || y + 1) - 1 - y) * 0.05));
        // zonas en penumbra: manchas suaves donde llega menos luz
        const zone = 0.42 + 0.58 * Math.min(1, Math.max(0, (fbm(x * 0.09 + 40, z * 0.09 + 11) - 0.32) / 0.4));
        // el pasto lejos de faroles y fogata queda más oscuro; cerca de una luz conserva su brillo
        if (tn === "grassTop") {
          let near = 0;
          LIGHTS.forEach(([lx, ly, lz, rad]) => {
            const d = Math.hypot(lx - x - 0.5, (ly - y) * 0.7, lz - z - 0.5);
            near = Math.max(near, 1 - Math.min(1, d / rad));
          });
          tint *= 0.3 + 0.7 * Math.sqrt(near);
        }
        const underPlates = Math.max(0, Math.min(1, (-(x + z - N) / N) * 1.4 + 0.3)); // 1 al fondo (debajo de las placas), 0 al frente
        const leafDim = tn === "leaves" || tn === "birchLeaves" ? 0.34 * (1 - 0.45 * underPlates) : 1;
        const v = (tn === "leaves" || tn === "birchLeaves" ? AOLEAF : AOL)[ao] * Fc.sh * tint * (Fc.n[1] === 1 ? zone : deep * (0.75 + 0.25 * zone)) * leafDim;
        b.c.push(v, v, v);
      });
      if (aos[0] + aos[2] < aos[1] + aos[3]) b.i.push(base + 1, base + 2, base + 3, base + 1, base + 3, base);
      else b.i.push(base, base + 1, base + 2, base, base + 2, base + 3);
    });
  });
  // plantas en cruz: pasto alto, flores
  function lightAt(x, y, z) {
    let near = 0;
    LIGHTS.forEach(([lx, ly, lz, rad]) => {
      const d = Math.hypot(lx - x - 0.5, (ly - y) * 0.7, lz - z - 0.5);
      near = Math.max(near, 1 - Math.min(1, d / rad));
    });
    return 0.3 + 0.65 * Math.sqrt(near);
  }
  function cross(tn, x, y, z, s) {
    const b = buf(tn), c = [x - half + 0.5, y, z - half + 0.5], q = 0.45 * (s || 1), hgt = s || 1;
    [[[-q, 0, -q], [q, 0, q]], [[-q, 0, q], [q, 0, -q]]].forEach(([a, e]) => {
      const base = b.p.length / 3;
      [[a, 0], [e, 0], [e, hgt], [a, hgt]].forEach(([p, yy], vi) => {
        b.p.push(c[0] + p[0], c[1] + yy, c[2] + p[2]);
        b.nn.push(0, 1, 0);
        b.uv.push([0, 1, 1, 0][vi], [0, 0, 1, 1][vi]);
        const k = lightAt(x, y, z);
        b.c.push(k, k, k);
      });
      b.i.push(base, base + 1, base + 2, base, base + 2, base + 3);
    });
  }
  Object.keys(top).forEach((k) => {
    const [i, j] = k.split(",").map(Number), t = top[k];
    if (get(i, t - 1, j) !== "grass" || get(i, t, j)) return;
    const v = h2(i * 3.1, j * 1.7);
    if (v < 0.28) cross("tallgrass", i, t, j, 0.9);
    else if (v < 0.31) cross("poppy", i, t, j, 0.8);
    else if (v < 0.34) cross("dandelion", i, t, j, 0.8);
  });

  function mat(tn) {
    if (tn === "roofTop") return new T.MeshBasicMaterial({ map: TX.roofTop, vertexColors: true, color: new T.Color(0.1169, 0.0734, 0.0461).multiplyScalar(1.12), fog: false }); // se ve #623426 tras la gradación
    const plant = ["tallgrass", "wheat", "poppy", "dandelion", "glass", "lantern"].includes(tn);
    const m = new T.MeshStandardMaterial({ map: TX[tn], vertexColors: true, roughness: tn === "glass" ? 0.15 : 0.9, metalness: 0, alphaTest: plant ? 0.5 : 0, transparent: tn === "glass", side: plant ? T.DoubleSide : T.FrontSide });
    if (tn === "glow") {
      m.emissive = new T.Color(1, 1, 1);
      m.emissiveMap = TX.glow;
      m.emissiveIntensity = 3.2;
    }
    if (tn === "leaves" || tn === "birchLeaves") {
      const lm = new T.MeshLambertMaterial({ map: TX[tn], vertexColors: true, emissive: new T.Color(0.004, 0.008, 0.02), color: new T.Color(0.78, 0.9, 1.15) });
      // luz envolvente: las caras que no miran a una luz no quedan negras (como hojas translúcidas)
      lm.onBeforeCompile = (sh) => {
        sh.vertexShader = sh.vertexShader.replace("#include <lights_lambert_vertex>", T.ShaderChunk.lights_lambert_vertex.split("dotNL = dot( geometry.normal, directLight.direction );").join("dotNL = dot( geometry.normal, directLight.direction )*.9+.1;"));
      };
      return lm;
    }
    if (tn === "glass") {
      m.emissive = lin(0xffb060);
      m.emissiveIntensity = 0.35;
    }
    return m;
  }
  Object.entries(Bf).forEach(([tn, b]) => {
    const g = new T.BufferGeometry();
    g.setAttribute("position", new T.Float32BufferAttribute(b.p, 3));
    g.setAttribute("normal", new T.Float32BufferAttribute(b.nn, 3));
    g.setAttribute("uv", new T.Float32BufferAttribute(b.uv, 2));
    g.setAttribute("color", new T.Float32BufferAttribute(b.c, 3));
    g.setIndex(b.i);
    const m = new T.Mesh(g, mat(tn));
    m.castShadow = tn !== "glass";
    m.receiveShadow = true;
    scene.add(m);
    if (m.material.alphaTest) m.customDepthMaterial = new T.MeshDepthMaterial({ depthPacking: T.RGBADepthPacking, map: TX[tn], alphaTest: 0.5 });
  });

  /* ---------- fogata de Minecraft animada (sin humo) ---------- */
  const FIRE = { value: 0 };
  if (campfire) {
    const [fx, fy, fz] = campfire, cx = fx - half + 0.5, cz = fz - half + 0.5, g = new T.Group();
    g.position.set(cx, fy, cz);
    g.scale.setScalar(1.6);
    const logMat = new T.MeshStandardMaterial({ map: TX.logSide, roughness: 0.9 });
    [[1, 0.26, 0.26, 0, 0.13, -0.3], [1, 0.26, 0.26, 0, 0.13, 0.3], [0.26, 0.26, 1, -0.3, 0.39, 0], [0.26, 0.26, 1, 0.3, 0.39, 0]].forEach(([w, h, d, x, y, z]) => {
      const m = new T.Mesh(new T.BoxGeometry(w, h, d), logMat);
      m.position.set(x, y, z);
      m.castShadow = true;
      m.receiveShadow = true;
      g.add(m);
    });
    const ember = new T.Mesh(new T.BoxGeometry(0.62, 0.06, 0.62), new T.MeshBasicMaterial({ color: new T.Color(3, 0.9, 0.15) }));
    ember.position.y = 0.05;
    g.add(ember);
    const flameMat = new T.ShaderMaterial({
      side: T.DoubleSide,
      transparent: false,
      uniforms: { uTime: FIRE },
      vertexShader: "varying vec2 vUv;void main(){vUv=uv;gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.);}",
      fragmentShader: `uniform float uTime;varying vec2 vUv;
        float hs(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
        float ns(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);return mix(mix(hs(i),hs(i+vec2(1,0)),f.x),mix(hs(i+vec2(0,1)),hs(i+vec2(1,1)),f.x),f.y);}
        void main(){
          vec2 p=(floor(vUv*16.)+.5)/16.;
          float n=ns(vec2(p.x*5.,p.y*3.5-uTime*2.6))*.6+ns(vec2(p.x*11.,p.y*7.-uTime*4.1))*.4;
          float shape=(1.-p.y)*1.75-abs(p.x-.5)*2.1;
          float f=shape-n*.55;
          if(f<.06) discard;
          vec3 c=mix(vec3(1.,.22,.02),vec3(1.,.62,.08),smoothstep(.06,.28,f));
          c=mix(c,vec3(1.,.85,.35),smoothstep(.45,.75,f));
          gl_FragColor=vec4(c*1.45,1.);}`,
    });
    [0, Math.PI / 2].forEach((rot) => {
      const fl = new T.Mesh(new T.PlaneGeometry(0.95, 1.4), flameMat);
      fl.position.y = 0.78;
      fl.rotation.y = rot + Math.PI / 4;
      g.add(fl);
    });
    scene.add(g);
    const fireLight = new T.PointLight(new T.Color(1, 0.45, 0.12), 4, 5.5, 2);
    fireLight.position.set(cx, fy + 1.6, cz);
    scene.add(fireLight);
  }

  /* ---------- luces: luz fría de las placas + cielo azul en las sombras ---------- */
  scene.add(new T.HemisphereLight(new T.Color(0.18, 0.34, 0.95), new T.Color(0.16, 0.08, 0.06), 1.15));
  const sun = new T.DirectionalLight(new T.Color(0.7, 0.84, 1), 1.35); // luz de las placas: fría, desde arriba
  sun.position.set(6, 42, 6);
  sun.castShadow = true;
  sun.shadow.mapSize.set(4096, 4096);
  Object.assign(sun.shadow.camera, { left: -38, right: 38, top: 38, bottom: -38, near: 1, far: 160 });
  sun.shadow.radius = 3.5;
  sun.shadow.bias = -0.0004;
  sun.shadow.normalBias = 0.03;
  scene.add(sun);
  const fill = new T.DirectionalLight(new T.Color(0.25, 0.42, 1), 0.78);
  fill.position.set(-6, 14, 30);
  scene.add(fill);
  const fill2 = new T.DirectionalLight(new T.Color(0.62, 0.56, 0.62), 0.95);
  fill2.position.set(30, 14, -6);
  scene.add(fill2);
  const plateLight = new T.PointLight(new T.Color(0.35, 0.7, 1), 7, 46, 1.3);
  plateLight.position.set(0, 26, 0);
  scene.add(plateLight);
  lamps.slice(0, 16).forEach(([x, y, z]) => {
    const pl = new T.PointLight(new T.Color(1, 0.55, 0.2), 6, 9, 2);
    pl.position.set(x - half + 0.5, y + 0.5, z - half + 0.5);
    scene.add(pl);
  });

  /* ---------- agua ---------- */
  const MS = 192, maskData = new Uint8Array(MS * MS * 4), mv = new Float32Array(MS * MS), EXT = N * 1.4;
  for (let y = 0; y < MS; y++)
    for (let x = 0; x < MS; x++) {
      const wx = ((x + 0.5) / MS) * 2 * EXT - EXT, wz = ((y + 0.5) / MS) * 2 * EXT - EXT;
      mv[y * MS + x] = isLand(Math.floor(wx + half), Math.floor(wz + half)) ? 1 : 0;
    }
  for (let p = 0; p < 8; p++) {
    const c = mv.slice();
    for (let y = 1; y < MS - 1; y++) for (let x = 1; x < MS - 1; x++) mv[y * MS + x] = (c[y * MS + x] * 2 + c[y * MS + x - 1] + c[y * MS + x + 1] + c[(y - 1) * MS + x] + c[(y + 1) * MS + x]) / 6;
  }
  for (let n = 0; n < MS * MS; n++) {
    const v = Math.min(255, mv[n] * 255 * 1.7) | 0;
    maskData[n * 4] = maskData[n * 4 + 1] = maskData[n * 4 + 2] = v;
    maskData[n * 4 + 3] = 255;
  }
  const maskTex = new T.DataTexture(maskData, MS, MS, T.RGBAFormat);
  maskTex.magFilter = maskTex.minFilter = T.LinearFilter;
  maskTex.needsUpdate = true;
  const NOISE = `float hs(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
  float ns(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);return mix(mix(hs(i),hs(i+vec2(1,0)),f.x),mix(hs(i+vec2(0,1)),hs(i+vec2(1,1)),f.x),f.y);}`;
  // luces de la isla que se reflejan en el agua (las más cercanas a la orilla + fogata)
  const WL = lamps
    .map(([x, y, z]) => [x - half + 0.5, y + 0.5, z - half + 0.5, 1])
    .sort((p, q) => Math.max(Math.abs(q[0]), Math.abs(q[2])) - Math.max(Math.abs(p[0]), Math.abs(p[2])))
    .slice(0, 7);
  if (campfire) WL.push([campfire[0] - half + 0.5, campfire[1] + 1, campfire[2] - half + 0.5, 1.4]);
  while (WL.length < 8) WL.push([0, -50, 0, 0]);
  const WATER_VS=`uniform float uTime;uniform float uAmp;uniform sampler2D uMask;uniform float uExt;uniform mat4 uTexMat;
      varying vec3 vPos;varying vec3 vN;varying float vH;varying float vM;varying vec4 vUvR;
      vec3 gw(vec2 dir,float st,float wl,float sp,vec3 p,float damp,inout vec3 ta,inout vec3 bi){
        float k=6.2831853/wl;vec2 d=normalize(dir);float f=k*(dot(d,p.xz)-sp*uTime);st*=uAmp*damp;float a=st/k;
        ta+=vec3(-d.x*d.x*st*sin(f),d.x*st*cos(f),-d.x*d.y*st*sin(f));bi+=vec3(-d.x*d.y*st*sin(f),d.y*st*cos(f),-d.y*d.y*st*sin(f));
        return vec3(d.x*a*cos(f),a*sin(f),d.y*a*cos(f));}
      void main(){vec3 p=(modelMatrix*vec4(position,1.)).xyz;vec3 ta=vec3(1,0,0),bi=vec3(0,0,1),q=p;
        float m=texture2D(uMask,(p.xz+uExt)/(2.*uExt)).r; vM=m;
        float damp=.3+.7*(1.-smoothstep(.0,.7,m));             // las olas se calman al llegar a la orilla
        q+=gw(vec2(1.,.3),.13,11.,1.1,p,damp,ta,bi);   // mar de fondo largo
        q+=gw(vec2(.6,1.),.11,7.,.9,p,damp,ta,bi);
        q+=gw(vec2(-.5,.9),.09,4.6,.75,p,damp,ta,bi);
        q+=gw(vec2(.9,-.6),.08,3.2,.65,p,damp,ta,bi);
        q+=gw(vec2(-.8,-.3),.06,2.2,.55,p,damp,ta,bi);
        q+=gw(vec2(.2,-1.),.05,1.5,.45,p,damp,ta,bi);   // picado fino
        vN=normalize(cross(bi,ta));vPos=q;vH=q.y-p.y;vUvR=uTexMat*vec4(p,1.);gl_Position=projectionMatrix*viewMatrix*vec4(q,1.);}`;
  const waterMat=new T.ShaderMaterial({transparent:true,depthWrite:false,
    uniforms:{uTime:{value:0},uAmp:{value:1},uMask:{value:maskTex},uExt:{value:EXT},uView:{value:cam.position.clone().sub(new T.Vector3(0,SEA,0)).normalize()},
      uLights:{value:WL.map(l=>new T.Vector4(...l))},uR1:{value:16.},uR2:{value:80.},uTexMat:{value:new T.Matrix4()},tRefl:{value:null}},
    vertexShader:WATER_VS,
    fragmentShader:`uniform float uTime,uExt,uR1,uR2;uniform vec3 uView;uniform vec4 uLights[8];uniform sampler2D tRefl;
      varying vec3 vPos;varying vec3 vN;varying float vH;varying float vM;varying vec4 vUvR;${NOISE}
      // ondulación con proporción corregida para la vista inclinada: se ven ondas, no rayas
      const vec2 LAT=vec2(.7071,-.7071), DEP=vec2(.7071,.7071);
      float rip(vec2 p){vec2 q=vec2(dot(p,LAT),dot(p,DEP)*.38)*1.6;
        return ns(q+vec2(uTime*.18,uTime*.25))*.55+ns(q*2.1-vec2(uTime*.22,-uTime*.15))*.3+ns(q*4.3+uTime*.3)*.15;}
      void main(){
        float e=.06,h0=rip(vPos.xz);
        vec2 gr=vec2(rip(vPos.xz+vec2(e,0))-h0,rip(vPos.xz+vec2(0,e))-h0)/e;
        vec3 n=normalize(mix(normalize(vN),vec3(0.,1.,0.),.55)+vec3(-gr.x,0.,-gr.y)*.05);
        vec3 v=normalize(uView);float nv=max(dot(n,v),0.);
        float fr=.02+.98*pow(1.-nv,5.);
        // reflejo de la isla (espejo real) con ondulación
        vec4 uvr=vUvR; uvr.xy+=vec2(gr.x+gr.y,gr.y-gr.x)*.006;
        vec4 refl=texture2DProj(tRefl,uvr);
        vec3 r=reflect(-v,n);
        vec3 sky=vec3(.025,.05,.15);
        float th=(22.-vPos.y)/max(r.y,.04);vec2 hp=vPos.xz+r.xz*th;sky+=vec3(.25,.7,1.15)*exp(-dot(hp,hp)/300.)*1.1;   // brillo de las placas
        vec3 mirror=refl.rgb*vec3(.62,.7,.85)+sky*(1.-refl.a*.7);   // reflejo algo más oscuro y azulado, como agua de noche
        float m=vM;
        vec3 deep=vec3(.002,.012,.045), mid=vec3(.008,.05,.13), shallow=vec3(.02,.16,.24);
        vec3 body=mix(deep,mid,smoothstep(.0,.25,m)); body=mix(body,shallow,smoothstep(.25,.8,m));
        float rw=clamp(.42+fr*1.2,0.,.92);
        vec3 col=mix(body,mirror,rw);
        // destellos cálidos de faroles y fogata
        for(int i=0;i<8;i++){vec4 L=uLights[i]; if(L.w<=0.) continue;
          vec3 ld=L.xyz-vPos; float dist=length(ld); ld/=dist;
          vec3 hv=normalize(ld+v); float sp=pow(max(dot(n,hv),0.),260.);
          col+=vec3(1.,.55,.2)*sp*L.w*1.4/(1.+dist*dist*.03);}
        // espuma solo en la orilla
        float br=ns(vPos.xz*2.2+uTime*.15);
        float band=smoothstep(.6,1.,sin(m*22.-uTime*1.4+br*4.));
        float foam=smoothstep(.45,.95,m)*(band*.7+smoothstep(.85,.98,m))*smoothstep(.35,.7,ns(vPos.xz*6.+vec2(uTime*.25,0.))+.25);
        col=mix(col,vec3(.6,.72,.85),clamp(foam,0.,1.)*.85);
        float fg=pow(smoothstep(uR1,uR2,length(vPos.xz)),.6);
        float baseA=mix(.84,.92,smoothstep(.0,.6,m))+foam*.1;
        gl_FragColor=vec4(col,clamp(baseA,0.,1.)*(1.-fg));}`
  });
  const sea=new T.Mesh(new T.PlaneGeometry(180,180,360,360),waterMat);
  waterMat.uniforms.uAmp.value=.3; sea.rotation.x=-Math.PI/2; sea.position.y=SEA-.12; sea.renderOrder=2; scene.add(sea);
  const SMALLW=[];
  // agua de río, canal y pozo: tranquila, con la misma superficie
  const smallWater=new T.ShaderMaterial({transparent:true,depthWrite:false,uniforms:waterMat.uniforms,vertexShader:WATER_VS.replace('uniform float uAmp;','const float uAmp=.25;'),fragmentShader:waterMat.fragmentShader});
  smallWater.uniforms=Object.assign({},waterMat.uniforms,{uR1:{value:999.},uR2:{value:1000.}});
  water.forEach(k=>{const [x,y,z]=k.split(',').map(Number);
    const p=new T.Mesh(new T.PlaneGeometry(1,1,2,2),smallWater);SMALLW.push(p);p.rotation.x=-Math.PI/2;p.position.set(x-half+.5,y+.85,z-half+.5);p.renderOrder=2;scene.add(p);});

  /* ---------- bruma y luciérnagas ---------- */
  function dotTex() {
    const c = new OffscreenCanvas(1, 1);
    c.width = c.height = 64;
    const g = c.getContext("2d");
    const gr = g.createRadialGradient(32, 32, 0, 32, 32, 32);
    gr.addColorStop(0, "rgba(255,255,255,1)");
    gr.addColorStop(0.3, "rgba(255,255,255,.35)");
    gr.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = gr;
    g.fillRect(0, 0, 64, 64);
    return new T.CanvasTexture(c);
  }
  const DT = dotTex();
  const FC = 70, fpos = new Float32Array(FC * 3), fs = [];
  for (let n = 0; n < FC; n++) {
    let a = r() * N - half, b = r() * N - half;
    if (a + b > -2) {
      const t = a;
      a = -b;
      b = -t;
    }
    fs.push([a, 12 + r() * 6, b, r() * 6.28]);
  }
  const fgeo = new T.BufferGeometry();
  fgeo.setAttribute("position", new T.BufferAttribute(fpos, 3));
  scene.add(new T.Points(fgeo, new T.PointsMaterial({ map: DT, color: new T.Color(2, 1.4, 0.6), size: 0.32, transparent: true, depthWrite: false, blending: T.AdditiveBlending, fog: false })));
  const mists = [];
  for (let n = 0; n < 0; n++) {
    const s = new T.Sprite(new T.SpriteMaterial({ map: DT, color: new T.Color(0.25, 0.3, 0.5), transparent: true, opacity: 0.1, depthWrite: false, fog: false }));
    s.userData = { a: (n / 22) * 6.283, rad: 30 + r() * 8 };
    s.scale.set(22, 5, 1);
    scene.add(s);
    mists.push(s);
  }

  /* ---------- posproceso: bloom + tonemapping y gradación tipo shaderpack ---------- */
  let composer=null;
  const GRADE={uniforms:{tDiffuse:{value:null},tScene:{value:null},uRes:{value:new T.Vector2(1,1)}},
    vertexShader:'varying vec2 vUv;void main(){vUv=uv;gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.);}',
    fragmentShader:`uniform sampler2D tDiffuse;uniform sampler2D tScene;varying vec2 vUv;
      vec3 aces(vec3 x){return clamp((x*(2.51*x+.03))/(x*(2.43*x+.59)+.14),0.,1.);}
      void main(){vec4 src=texture2D(tDiffuse,vUv); vec3 c=src.rgb*.95;
        c=aces(c);
        float l=dot(c,vec3(.2126,.7152,.0722));
        c=mix(vec3(l),c,1.35); c=(c-.5)*1.1+.5;                                   // saturación
        c+=vec3(.0,.005,.02)*smoothstep(.4,1.,l);              // luces cálidas
        c+=vec3(-.012,.004,.05)*(1.-smoothstep(0.,.45,l))*smoothstep(0.,.05,l);          // sombras frías
        c=pow(max(c,0.),vec3(1./2.2));
        float a=texture2D(tScene,vUv).a;   // alfa real de la escena (antes del bloom): el mundo queda en su propia capa
        gl_FragColor=vec4(c,a);}`};   // premultiplicado: donde no hay mundo es transparente y el brillo solo se suma   // alfa premultiplicado: el fondo se ve detrás, el brillo se suma
  if(T.EffectComposer&&T.UnrealBloomPass){
    const rt=new T.WebGLRenderTarget(16,16,{type:T.HalfFloatType,minFilter:T.LinearFilter,magFilter:T.LinearFilter,format:T.RGBAFormat});
    composer=new T.EffectComposer(renderer,rt);
    composer.addPass(new T.RenderPass(scene,cam));
    const sceneRT=new T.WebGLRenderTarget(16,16,{type:T.HalfFloatType,format:T.RGBAFormat});
    const save=T.SavePass?new T.SavePass(sceneRT):null; if(save) composer.addPass(save);
    composer.addPass(new T.UnrealBloomPass(new T.Vector2(512,512),.5,.7,1.1));
    const gp=new T.ShaderPass(GRADE); gp.uniforms.tScene.value=sceneRT.texture; composer.addPass(gp);
    composer._sceneRT=sceneRT;
  }else{ renderer.outputEncoding=T.sRGBEncoding; renderer.toneMapping=T.ACESFilmicToneMapping; }

  // the camera follows where the plates are drawn, relative to the canvas (not the window)
  // Only does work when something changed: setSize reallocates the canvas and every render target
  // (bloom, scene alpha), which made each opening of the section stutter. The shadows don't depend
  // on the canvas, so they are never redone here.
  let lastSize = "", lastCam = "";
  const viewSize = { w: 0, h: 0 };
  function resize() {
    const b = { left: 0, top: 0, width: layout.width, height: layout.height }, st = { left: layout.stageLeft, top: layout.stageTop, width: layout.stageWidth }, pr = Math.min(layout.dpr || 1, 1.5);
    if (b.width < 2 || b.height < 2 || st.width < 2) return;
    const sc = st.width / 1000, ox = st.left - b.left + 500 * sc, oy = st.top - b.top + Y0 * sc, u = K * sc;
    const camKey = `${ox.toFixed(2)}|${oy.toFixed(2)}|${u.toFixed(4)}|${b.width}|${b.height}`;
    if (camKey !== lastCam) {
      lastCam = camKey;
      cam.left = -ox / u;
      cam.right = (b.width - ox) / u;
      cam.top = oy / u;
      cam.bottom = -(b.height - oy) / u;
      cam.updateProjectionMatrix();
    }
    const sizeKey = `${b.width}x${b.height}@${pr}`;
    if (sizeKey === lastSize) return;
    lastSize = sizeKey;
    viewSize.w = b.width;
    viewSize.h = b.height;
    renderer.setPixelRatio(pr);
    renderer.setSize(b.width, b.height, false);
    if (composer) {
      if (composer.setPixelRatio) composer.setPixelRatio(pr);
      composer.setSize(b.width, b.height);
      if (composer._sceneRT) composer._sceneRT.setSize(Math.round(b.width * pr), Math.round(b.height * pr));
    }
  }

  /* ---------- reflejo: la escena vista en espejo desde debajo del agua ---------- */
  const reflRT=new T.WebGLRenderTarget(16,16,{type:T.HalfFloatType,format:T.RGBAFormat,minFilter:T.LinearFilter,magFilter:T.LinearFilter});
  const vcam=cam.clone(), mirrorY=SEA-.12, clipPlane=new T.Plane(new T.Vector3(0,1,0),-mirrorY);
  const BIAS=new T.Matrix4().set(.5,0,0,.5, 0,.5,0,.5, 0,0,.5,.5, 0,0,0,1);
  waterMat.uniforms.tRefl.value=reflRT.texture;
  renderer.shadowMap.autoUpdate=false; renderer.shadowMap.needsUpdate=true;   // la escena es estática: sombras una sola vez
  function renderReflection(){
    const b={width:viewSize.w,height:viewSize.h}, pr=Math.min(layout.dpr||1,1.5)*.6;   // size kept by resize(): no layout read per frame
    const W2=Math.max(16,Math.round(b.width*pr)), H2=Math.max(16,Math.round(b.height*pr));
    if(reflRT.width!==W2||reflRT.height!==H2) reflRT.setSize(W2,H2);
    vcam.left=-cam.right; vcam.right=-cam.left; vcam.top=cam.top; vcam.bottom=cam.bottom; vcam.near=cam.near; vcam.far=cam.far;
    vcam.position.set(cam.position.x,2*mirrorY-cam.position.y,cam.position.z);
    vcam.up.set(0,-1,0); vcam.lookAt(0,mirrorY,0); vcam.updateProjectionMatrix(); vcam.updateMatrixWorld();
    waterMat.uniforms.uTexMat.value.copy(BIAS).multiply(vcam.projectionMatrix).multiply(vcam.matrixWorldInverse);
    sea.visible=false; SMALLW.forEach(m=>m.visible=false);
    renderer.clippingPlanes=[clipPlane];
    const cc=renderer.getClearColor(new T.Color()), ca=renderer.getClearAlpha();
    renderer.setRenderTarget(reflRT); renderer.setClearColor(0x000000,0); renderer.clear(); renderer.render(scene,vcam);
    renderer.setRenderTarget(null); renderer.setClearColor(cc,ca); renderer.clippingPlanes=[];
    sea.visible=true; SMALLW.forEach(m=>m.visible=true);
  }

  function draw(t) {
    try {
      renderReflection();
    } catch (e) {
      console.warn("reflejo:", e);
    }
    FIRE.value = t;
    waterMat.uniforms.uTime.value = t;
    for (let n = 0; n < FC; n++) {
      const s = fs[n];
      fpos[n * 3] = s[0] + Math.sin(t * 0.3 + s[3]) * 0.8;
      fpos[n * 3 + 1] = s[1] + Math.sin(t * 0.5 + s[3] * 2) * 0.5;
      fpos[n * 3 + 2] = s[2] + Math.cos(t * 0.25 + s[3]) * 0.8;
    }
    fgeo.attributes.position.needsUpdate = true;
    mists.forEach((s) => {
      const a = s.userData.a + t * 0.008;
      s.position.set(Math.cos(a) * s.userData.rad, SEA + 0.8, Math.sin(a) * s.userData.rad);
    });
    if (composer) composer.render();
    else renderer.render(scene, cam);
  }
  return { resize, draw };
}

let world = null, pendingT = 0, raf = 0;
function frame() {
  raf = 0;
  world.draw(pendingT);
}

self.onmessage = ({ data: m }) => {
  if (m.type === "init") {
    try {
      importScripts(...THREE_FILES.map((file) => `/vendor/three-r128/${file}`));
      ({ BASE_Y, HALF_W, HALF_H } = m.consts);
      layout = m.layout;
      world = buildWorld(self.THREE, m.canvas);
      if (!world) throw new Error("WebGL no disponible");
      world.resize();
      world.draw(2);
      self.postMessage({ type: "ready" });
    } catch (err) {
      world = null;
      self.postMessage({ type: "failed", error: String(err) });
    }
  } else if (m.type === "layout") {
    layout = m.layout;
    if (!world) return;
    world.resize();
    // resizing clears the canvas: repaint even if the section is not animating right now
    if (!raf) raf = requestAnimationFrame(frame);
  } else if (m.type === "draw") {
    pendingT = m.t;
    if (world && !raf) raf = requestAnimationFrame(frame);
  }
};
