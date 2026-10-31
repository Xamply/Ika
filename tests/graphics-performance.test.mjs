import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");
const extract = (code, start, end) => code.slice(code.indexOf(start), code.indexOf(end, code.indexOf(start)));

test("deferred scene resume presents cached frame first and cancels rapid returns", () => {
  const frames = new Map(), timers = new Map(); let id = 0, ticks = 0;
  const context = vm.createContext({ document: { hidden: false, addEventListener() {} },
    performance: { now: () => 100 }, console,
    requestAnimationFrame: (fn) => { frames.set(++id, fn); return id; },
    cancelAnimationFrame: (key) => frames.delete(key),
    setTimeout: (fn) => { timers.set(++id, fn); return id; }, clearTimeout: (key) => timers.delete(key),
  });
  vm.runInContext(source("frame-clock.js").replaceAll("export ", ""), context);
  const fire = () => { const [key, fn] = frames.entries().next().value; frames.delete(key); fn(100); };
  for (let n = 0; n < 100; n++) { const stop = context.onTickAfterPaint(() => ticks++); stop(); }
  assert.equal(frames.size, 0);
  let stop = context.onTickAfterPaint(() => ticks++);
  fire(); stop(); assert.equal(frames.size, 0);
  stop = context.onTickAfterPaint(() => ticks++);
  fire(); fire(); assert.equal(ticks, 0);
  fire(); assert.equal(ticks, 1);
  stop(); assert.equal(frames.size + timers.size, 0);
});

test("sea skips invisible triangles while preserving original visible grid", () => {
  const context = vm.createContext({});
  vm.runInContext(extract(source("manager-world.worker.js"), "function trimWaterGeometry(", "/* ========================================================="), context);
  const points = [], indices = [], segments = 360;
  for (let y = 0; y <= segments; y++) for (let x = 0; x <= segments; x++) points.push([x * .5 - 90, y * .5 - 90]);
  for (let y = 0; y < segments; y++) for (let x = 0; x < segments; x++) {
    const a = y * (segments + 1) + x, b = a + 1, c = a + segments + 1, d = c + 1;
    indices.push(a, b, c, b, d, c);
  }
  const geometry = { attributes: { position: { getX: (i) => points[i][0], getY: (i) => points[i][1] } },
    index: { array: indices }, setIndex(value) { this.kept = value; } };
  context.trimWaterGeometry(geometry);
  assert.ok(geometry.kept.length < indices.length * .4);
  const kept = new Set();
  for (let n = 0; n < geometry.kept.length; n += 3) kept.add(geometry.kept.slice(n, n + 3).join(","));
  for (let n = 0; n < indices.length; n += 3) {
    const tri = indices.slice(n, n + 3);
    const side = tri.map((i) => (points[i][0] + points[i][1]) * Math.SQRT1_2);
    if (Math.min(...side) <= 24.5 + .15 && Math.max(...side) >= -24.5 - .15) assert.ok(kept.has(tri.join(",")));
  }
});

test("comparator cards survive 1000 returns and update after metadata or pending scan changes", () => {
  let writes = 0;
  const nodes = new Map(), sides = { a: { mods_path: "mods", name: "One", loader: "Fabric", mc_version: "1.21", mod_count: 10 } };
  const pendingScan = new Map();
  const context = vm.createContext({ sides, pendingScan, nameOf: (i) => i.name, metaOf: (i) => `${i.loader}|${i.mc_version}`,
    getInstanceCoverUrl: () => "cover.jpg", modCountOf: () => 10, escapeHtml: (s) => s,
    $: (key) => {
      if (!nodes.has(key)) nodes.set(key, { classList: { toggle() { writes++; } }, style: {},
        set textContent(v) { writes++; }, set innerHTML(v) { writes++; }, getAttribute() { return null; }, setAttribute() { writes++; } });
      return nodes.get(key);
    },
  });
  vm.runInContext(extract(source("main.js"), "  const panStates =", "  function dropPan("), context);
  context.renderPan("a"); const first = writes;
  for (let n = 0; n < 1000; n++) context.renderPan("a");
  assert.equal(writes, first);
  sides.a.name = "Renamed"; context.renderPan("a"); assert.ok(writes > first);
  const renamed = writes; pendingScan.set("mods", {}); context.renderPan("a"); assert.ok(writes > renamed);
});

test("comparator hot-zone noise is evaluated once per side per timestamp", () => {
  let calls = 0;
  const context = vm.createContext({ vnoise: () => { calls++; return .2; }, gauss: (x, c, s) => Math.exp(-((x-c)**2)/(2*s*s)) });
  vm.runInContext(extract(source("comparador-keo.js"), "    const HOT =", "    const CROSS ="), context);
  for (const side of [-1, 1]) for (let n = 0; n < 1000; n++) {
    const th = n / 1000;
    const expected = .4 + .6 * Math.min(1, Math.exp(-((th - (side < 0 ? .85 : 1.3) - .45*.2)**2)/(2*.42**2)) +
      Math.exp(-((th - (side < 0 ? 2.5 : 2.45) - .4*.2)**2)/(2*.36**2)) + .35*Math.exp(-((th-.05)**2)/(2*.12**2)));
    assert.ok(Math.abs(context.gmap(side, th, 5) - expected) < 1e-12);
  }
  assert.equal(calls, 4);
  context.gmap(-1, 1, 6); assert.equal(calls, 6);
});

test("cached fibre samples preserve animated coordinates and envelopes", () => {
  let rendered;
  const noise = (x, y) => Math.sin(x * .7 + y * .3) * .3;
  const flow = (x, y, t, ox, oy, sp = 1) => noise(x + ox + t, y + oy + t * sp);
  const fire = (x, y, t, ox, oy, sp = 1) => noise(x * .2 + ox + t, y * .2 + oy + t * sp);
  const context = vm.createContext({ CX: 816.5, CY: 110, PI: Math.PI, vnoise: noise, flow, fire,
    clamp01: (v) => Math.max(0, Math.min(1, v)), gmap: () => .7, G: { ring: {}, ringHot: {} },
    fiber: (pts, env) => { rendered = { pts: pts.map((p) => [...p]), env: [...env] }; },
  });
  vm.runInContext(extract(source("comparador-keo.js"), "    function sideFibre(", "    const TONG ="), context);
  for (const full of [false, true]) for (const side of [-1, 1]) {
    const s = { th0: .05, th1: 2.9, side, full, ox: 4, oy: 8, pv: .1, A1: 5, A2: 2, sp: 1.1, L: 3, r0: 92, tip: 6, bot: 4, w: 1, a: .6 };
    for (const t of [0, 1.2, 100]) {
      context.sideFibre(s, t, .8);
      const points = s.points, samples = s.samples;
      const n = Math.max(10, Math.round((s.th1 - s.th0) / .035));
      for (let k = 0; k <= n; k++) {
        const u = k / n, th = s.th0 + (s.th1 - s.th0) * u, bx = 816.5 + side * 90 * Math.sin(th), by = 110 - 90 * Math.cos(th);
        const conv = Math.pow(Math.sin(th), .7), spread = .5 + Math.abs(Math.sin(2 * th));
        const amp = s.A1 * .8 * (.55 + .8 * Math.max(0, Math.min(1, .5 + noise(t * .08, s.ox + 7))));
        const r = 90 + (s.r0 - 90) * conv + conv * spread * (s.A2 * flow(bx, by, t, 3.1, 7.3) + amp * flow(bx, by, t, s.ox, s.oy, s.sp)) +
          conv * Math.pow(Math.max(0, flow(bx, by, t, s.ox + 40, s.oy, 1.25) - .15), 2) * s.L * 2.6 * .8;
        assert.ok(Math.abs(rendered.pts[k][0] - (816.5 + side * r * Math.sin(th))) < 1e-10);
        assert.ok(Math.abs(rendered.pts[k][1] - (110 - r * Math.cos(th) - s.tip * Math.exp(-th/.2) + s.bot * Math.exp(-(Math.PI-th)/.25))) < 1e-10);
        const taper = full ? Math.min(1, u/.05, (1-u)/.05) : Math.pow(Math.sin(Math.PI*u), .9);
        const hot = Math.pow(Math.max(0, Math.min(1, .5 + .8 * fire(bx, by, t, s.ox+80, s.oy+20, 1.1))), 1.4);
        assert.ok(Math.abs(rendered.env[k] - Math.max(0, taper) * (.2 + .8*hot) * .7 * (.75 + .25*noise(t*2.2, s.ox))) < 1e-10);
      }
      context.sideFibre(s, t, .8);
      assert.equal(s.points, points); assert.equal(s.samples, samples);
    }
  }
});
