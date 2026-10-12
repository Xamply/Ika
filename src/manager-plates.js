// Gestor de Instancia: the 4 holographic plates (SVG) stacked over a Minecraft island (three.js r128).
// Ported from placas.html. Plates, top to bottom: Resource packs, Shaders, Mods, Configuración;
// the island underneath is the base (Minecraft itself).
// The plates are drawn here (as layers that are drawn once and then only moved: see buildPlates);
// the island is drawn by a worker (manager-world.worker.js).

const NS = "http://www.w3.org/2000/svg";
function el(tag, attrs, parent) {
  const e = document.createElementNS(NS, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (parent) parent.appendChild(e);
  return e;
}
function rng(a) {
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/* ===== Configuración: colores, posiciones y tamaño ===== */
const W = 1000, H = 1000, CX = 500;
const HALF_W = 392; // mitad del ancho de cada placa
const HALF_H = 96; // mitad del alto (profundidad aparente)
const SKEW = 0; // 0 = placas simétricas
const FIT = 0.62; // 1 = proporción original (más zoom); menor = se ve más de la foto
const RADIUS = 0; // esquinas rectas

const PHOTOS = {
  overworld: "/manager/plate-overworld.jpg",
  nether: "/manager/plate-nether.jpg",
  end: "/manager/plate-end.jpg",
  sift: "/manager/plate-sift.jpg",
};

// de arriba hacia abajo
export const PLATES = [
  { key: "packs", y: 225, glow: "#7dff2a", edge: "#d4ff8a", alt: "#3fbf1a", peaks: ["#c8ff4d", "#e6ffb0", "#9cff3a"], src: "overworld", seed: 3 },
  { key: "shaders", y: 350, glow: "#ff3b2f", edge: "#ff9a3c", alt: "#9b2cff", peaks: ["#ff7a1f", "#ffb347", "#ff3d6e", "#c13cff"], src: "nether", seed: 11 },
  { key: "mods", y: 475, glow: "#8a2cff", edge: "#2f6bff", alt: "#5a1fd6", peaks: ["#b04dff", "#3f7dff", "#9a45ff", "#2a5cff"], src: "end", imgY: 30, seed: 23 },
  { key: "config", y: 600, glow: "#5cd2ff", edge: "#bff0ff", alt: "#3fa9ff", peaks: ["#d6f7ff", "#9ee6ff", "#e9fbff"], src: "sift", seed: 41 },
];
// Where the island's base sits in the 1000×1000 plate space
export const BASE_Y = 905;

/* ===== Brillo variable a lo largo del borde ===== */
function edgeStops(grad, p, seed) {
  // contraste alto: zonas casi negras, medias de color y picos blancos; el último tramo repite el primero para que el ciclo no tenga salto
  const r = rng(seed), n = 10, pts = [];
  for (let k = 0; k < n; k++) {
    const v = r(), b = v < 0.35 ? 0.02 + r() * 0.06 : v < 0.65 ? 0.45 + r() * 0.2 : 1;
    const col = b === 1 ? p.peaks[(r() * p.peaks.length) | 0] : b > 0.3 ? p.glow : p.alt;
    pts.push([col, b]);
  }
  pts.push(pts[0]);
  pts.forEach(([col, b], k) => el("stop", { offset: (k / n).toFixed(3), "stop-color": col, "stop-opacity": b.toFixed(2) }, grad));
}

function h1(n) {
  n = Math.sin(n * 127.1 + 311.7) * 43758.5453;
  return n - Math.floor(n);
}
function vn(x) {
  const i = Math.floor(x), f = x - i, u = f * f * (3 - 2 * f);
  return h1(i) + (h1(i + 1) - h1(i)) * u;
}

/* ===== Ondas de energía con WebGL (wavesGl) ===== */
// A wave is a ribbon of triangles along its line: a_at is the point of the line, a_side goes
// across it (to one side or the other, longer at the corners). Positions in the 1000×1000 space.
const WAVE_VS = `#version 300 es
in vec2 a_at;
in vec2 a_side;
in float a_alpha;
uniform vec2 u_size;  // of what is drawn on, px
uniform float u_sc;   // px per unit there
uniform float u_half; // half the line's width
uniform float u_lift; // how far the plate is from its place (floating, chosen)
out vec2 v_at;
out float v_alpha;
void main() {
  v_at = a_at + a_side * u_half;
  v_alpha = a_alpha;
  vec2 px = vec2(v_at.x, v_at.y + u_lift) * u_sc;
  gl_Position = vec4(px.x / u_size.x * 2.0 - 1.0, 1.0 - px.y / u_size.y * 2.0, 0.0, 1.0);
}`;
// Only inside the plate's rhombus (u_plate: its centre), with a soft pixel at its edge.
const WAVE_FS = `#version 300 es
precision highp float;
in vec2 v_at;
in float v_alpha;
uniform vec2 u_plate;
uniform vec3 u_color;
uniform float u_layer; // the opacity of the layer (glow or core)
uniform float u_sc;
out vec4 o;
void main() {
  vec2 d = abs(v_at - u_plate) / vec2(${HALF_W}.0, ${HALF_H}.0);
  float inside = (1.0 - d.x - d.y) * ${((HALF_W * HALF_H) / Math.hypot(HALF_W, HALF_H)).toFixed(3)};
  float a = v_alpha * u_layer * clamp(inside * u_sc + 0.5, 0.0, 1.0);
  o = vec4(u_color * a, a);
}`;
// One triangle that covers everything, to pass a picture through the blur.
const FULL_VS = `#version 300 es
out vec2 v_uv;
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  v_uv = p * 0.5 + 0.5;
  gl_Position = vec4(p, 0.0, 1.0);
}`;
// Gaussian blur along one direction (u_step: one pixel that way), u_sigma in px.
const BLUR_FS = `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec2 u_step;
uniform float u_sigma;
out vec4 o;
void main() {
  vec4 sum = texture(u_tex, v_uv);
  float total = 1.0;
  for (int i = 1; i <= 16; i++) {
    float x = float(i);
    if (x > u_sigma * 3.0 + 1.0) break;
    float w = exp(-x * x / (2.0 * u_sigma * u_sigma));
    sum += (texture(u_tex, v_uv + u_step * x) + texture(u_tex, v_uv - u_step * x)) * w;
    total += 2.0 * w;
  }
  o = sum / total;
}`;

// Builds the plates inside `stage` as a stack of layers: one <svg> each, all on the same
// 1000×1000 space. A plate is three of them:
//   face  - its shadow, its photo and the light on it. It never changes: the browser draws it once
//           and from then on only moves that picture (floating, rising when chosen).
//   halo  - the wide glows of its edges. They follow the light that runs along the edges, but that
//           blurred nobody can tell when: drawn again only a few times per second.
//   line  - the thin lines of the edges, where the running light is seen. Drawn every frame, which
//           is cheap: thin strokes with little or no blur.
// As ONE svg every frame redrew everything (48 blurred shapes, some very wide), because moving a
// group inside an svg repaints it. The first and the last <svg> (.mgr-plates-svg) are not plates:
// the first holds the stars and what the others refer to (filters, gradients, masks), the last the
// transparent diamonds that take the pointer (.mgr-plate-hit).
function buildPlates(stage) {
  const layerSvg = (cls, attrs = {}) => el("svg", { viewBox: `0 0 ${W} ${H}`, class: cls, ...attrs }, stage);
  const svg = layerSvg("mgr-plates-svg", { role: "img", "aria-label": "Capas de la instancia: resource packs, shaders, mods y configuración sobre Minecraft" });
  const defs = el("defs", {}, svg);
  // The filter region is the element's own box plus 3.2σ of blur (+ stroke) on each side. The
  // design used one 1400×1400 region for every blurred element: same picture, but every glow was
  // blurred over the whole canvas. The smallest filtered boxes are ~784×96 (the V-shaped edges),
  // which sets the fractions.
  const blur = (id, sd) => {
    const mx = 0.03 + (3.2 * sd + 12) / 784, my = 0.06 + (3.2 * sd + 12) / 96;
    const f = el("filter", { id, filterUnits: "objectBoundingBox", x: -mx, y: -my, width: 1 + 2 * mx, height: 1 + 2 * my }, defs);
    el("feGaussianBlur", { stdDeviation: sd }, f);
  };
  blur("mgrb0", 0.9);
  blur("mgrb1", 1.6);
  blur("mgrb2", 4);
  blur("mgrb3", 10);
  blur("mgrb4", 22);
  blur("mgrb5", 40);

  // estrellas de fondo
  const sr = rng(7);
  for (let i = 0; i < 90; i++) el("circle", { cx: sr() * W, cy: sr() * H, r: sr() * 1.1 + 0.2, fill: "#aab4ff", opacity: (0.15 + sr() * 0.5).toFixed(2) }, svg);

  const ns = { "vector-effect": "non-scaling-stroke" };
  const ANIM = [], WAVES = [];
  const groups = PLATES.map(() => []); // the layers of each plate, to float / lift it
  const plates = [...PLATES].reverse(); // se pinta de abajo hacia arriba
  plates.forEach((p) => {
    const i = PLATES.indexOf(p), id = "mgrp" + i;
    const L = { x: CX - HALF_W, y: p.y }, B = { x: CX - SKEW, y: p.y - HALF_H }, F = { x: CX + SKEW, y: p.y + HALF_H }, R = { x: CX + HALF_W, y: p.y };
    const M = `matrix(${(B.x - L.x) / 100} ${(B.y - L.y) / 100} ${(F.x - L.x) / 100} ${(F.y - L.y) / 100} ${L.x} ${L.y})`;
    const rect = (parent, attrs) => el("rect", { x: 0, y: 0, width: 100, height: 100, rx: RADIUS, ry: RADIUS, transform: M, "stroke-linejoin": "round", "stroke-linecap": "round", ...attrs }, parent);
    const layer = (cls) => {
      const s = layerSvg(`mgr-plate-layer ${cls}`, { "data-p": i, "aria-hidden": "true" });
      groups[i].push(s);
      return s;
    };

    // degradado A: recorre la placa de izquierda a derecha
    const ga = el("linearGradient", { id: id + "A", spreadMethod: "repeat", gradientUnits: "userSpaceOnUse", x1: 0, y1: 0, x2: 100, y2: 100 }, defs);
    edgeStops(ga, p, p.seed);
    // the same one for the halos, which are moved on their own (less often)
    const gh = el("linearGradient", { id: id + "H", spreadMethod: "repeat", gradientUnits: "userSpaceOnUse", x1: 0, y1: 0, x2: 100, y2: 100 }, defs);
    edgeStops(gh, p, p.seed);
    // degradado C: recorre de la esquina izquierda a la trasera, rompe la simetría entre bordes
    const gc = el("linearGradient", { id: id + "C", spreadMethod: "repeat", gradientUnits: "userSpaceOnUse", x1: 0, y1: 0, x2: 100, y2: 0 }, defs);
    edgeStops(gc, p, p.seed * 7 + 5);
    ANIM.push({ ga, gc, gh, speed: 2.2 + i * 0.5, dir: i % 2 ? -1 : 1, phase: i * 23 });
    // máscara: el fondo de la placa se apaga, el frente se enciende
    const mg = el("linearGradient", { id: id + "M", gradientUnits: "userSpaceOnUse", x1: 0, y1: B.y - 20, x2: 0, y2: F.y + 20 }, defs);
    // desde la segunda placa, la sombra de la placa de arriba también oscurece los bordes traseros
    (i > 0 ? [[0, 0.22], [0.3, 0.4], [0.48, 0.85], [0.55, 1], [1, 1]] : [[0, 0.35], [0.55, 0.75], [1, 1]]).forEach(([o, a]) => el("stop", { offset: o, "stop-color": "#fff", "stop-opacity": a }, mg));
    const mask = el("mask", { id: id + "mask", maskUnits: "userSpaceOnUse", x: 0, y: 0, width: W, height: H }, defs);
    el("rect", { x: 0, y: 0, width: W, height: H, fill: `url(#${id}M)` }, mask);
    const cp = el("clipPath", { id: id + "clip" }, defs);
    el("rect", { x: 0, y: 0, width: 100, height: 100, rx: RADIUS, ry: RADIUS }, cp);
    const cps = el("clipPath", { id: id + "clipS", clipPathUnits: "userSpaceOnUse" }, defs);
    el("rect", { x: 0, y: 0, width: 100, height: 100, rx: RADIUS, ry: RADIUS, transform: M }, cps);

    // ---- face: everything that never changes ----
    const face = layer("is-face");
    // sombra bajo la V: oscura pegada al borde y degradándose hacia abajo (también en la última placa)
    {
      const ds = el("g", {}, face);
      const band = (o, op, f) => el("polygon", { points: `${L.x},${L.y} ${F.x},${F.y} ${R.x},${R.y} ${R.x - 60},${R.y + o * 0.35} ${F.x},${F.y + o} ${L.x + 60},${L.y + o * 0.35}`, fill: "#020008", opacity: op, filter: `url(#${f})` }, ds);
      const last = i === PLATES.length - 1, k = last ? 1.6 : 1; // la última cae sobre el fondo: más larga y marcada
      band(44 * k, last ? 0.6 : 0.4, "mgrb4"); // parte baja, muy difusa
      band(22 * k, last ? 0.8 : 0.6, "mgrb3"); // intermedia
      band(9 * k, 0.9, "mgrb2"); // pegada al borde, la más oscura
    }
    const gPhoto = el("g", {}, face);
    // resplandor interior en modo superposición, color de la placa, se apaga antes del centro. It
    // only reaches the photo under it (it is clipped to the plate), so it can blend inside the layer.
    {
      const ov = el("g", { "clip-path": `url(#${id}clipS)`, style: "mix-blend-mode:overlay" }, face);
      rect(el("g", { filter: "url(#mgrb3)", opacity: 0.75 }, ov), { fill: "none", stroke: p.glow, "stroke-width": 44, ...ns });
      rect(el("g", { filter: "url(#mgrb2)" }, ov), { fill: "none", stroke: p.glow, "stroke-width": 14, ...ns });
    }
    // la foto se queda recta: solo se recorta con la forma de la placa, sin inclinarla; ocupa todo
    // el ancho de la placa y se comprime un poco en vertical (FIT) para que se vea más escena
    {
      const iw = R.x - L.x, ih = iw * (559 / 1024) * FIT;
      el("image", { href: PHOTOS[p.src], x: L.x, y: p.y - ih / 2 + (p.imgY || 0), width: iw, height: ih, preserveAspectRatio: "none", "clip-path": `url(#${id}clipS)` }, gPhoto);
    }

    // sombra de la placa de arriba: degradado oscuro que baja desde el vértice superior y se desvanece
    if (i > 0) {
      const sg = el("linearGradient", { id: id + "sh", gradientUnits: "userSpaceOnUse", x1: CX, y1: B.y, x2: CX, y2: F.y }, defs);
      [[0, 0.75], [0.2, 0.55], [0.4, 0.28], [0.58, 0.08], [0.7, 0]].forEach(([o, a]) => el("stop", { offset: o, "stop-color": "#04020c", "stop-opacity": a }, sg));
      const sh = el("g", { "clip-path": `url(#${id}clipS)` }, face);
      el("rect", { x: L.x, y: B.y, width: R.x - L.x, height: F.y - B.y, fill: `url(#${id}sh)` }, sh);
    }
    // más exposición en la zona iluminada (la que no cubre la sombra)
    {
      const lg = el("linearGradient", { id: id + "lit", gradientUnits: "userSpaceOnUse", x1: CX, y1: B.y, x2: CX, y2: F.y }, defs);
      const k = i > 0 ? 0.7 : 0.45;
      el("stop", { offset: 0, "stop-color": "#fff", "stop-opacity": 0 }, lg);
      el("stop", { offset: i > 0 ? 0.5 : 0.3, "stop-color": "#fff", "stop-opacity": 0 }, lg);
      el("stop", { offset: 0.8, "stop-color": "#fff", "stop-opacity": 0.12 * k }, lg);
      el("stop", { offset: 1, "stop-color": "#fff", "stop-opacity": 0.28 * k }, lg);
      const lit = el("g", { "clip-path": `url(#${id}clipS)`, style: "mix-blend-mode:screen" }, face);
      el("polygon", { points: `${L.x},${L.y} ${B.x},${B.y} ${R.x},${R.y} ${F.x},${F.y}`, fill: `url(#${id}lit)`, filter: "url(#mgrb2)" }, lit);
    }

    // ondas de energía sobre los bordes frontales (la V). Same data as the design (same random
    // sequence), but drawn on a canvas over the plates (paintWaves): as 20 SVG paths of 300 points
    // with blur filters, rebuilt every frame, they made the browser re-rasterize 40 filters per frame.
    {
      const cols = [p.glow, p.edge, ...p.peaks];
      for (let k = 0; k < 5; k++) {
        const c = cols[k % cols.length];
        const r = rng(p.seed * 13 + k);
        // tramos que van de casi transparente a un brillo moderado, nunca uniforme
        const stops = [];
        for (let q = 0; q <= 9; q++) {
          const v = r();
          stops.push({ o: q / 9, color: k === 0 ? p.edge : c, a: +(v < 0.4 ? 0.03 + r() * 0.07 : v < 0.8 ? 0.18 + r() * 0.15 : 0.45 + r() * 0.15).toFixed(2) });
        }
        WAVES.push({ i, k, stops, drift: (8 + r() * 10) * (k % 2 ? -1 : 1), L, F, R, B, base: 2 + k * 1.8, amp: 5 + r() * 4, fine: 0.3 + r() * 0.3, jag: 0.7 + r() * 0.5, spike: 1.2 + r() * 1, speed: 0.18 + r() * 0.12, ph: r() * 100 });
      }
    }

    // ---- halo: the wide glows of the edges ----
    const halo = el("g", { mask: `url(#${id}mask)` }, layer("is-glow"));
    // brillo interior que nace del borde
    const inner = el("g", { filter: "url(#mgrb3)", opacity: 0.5 }, halo);
    el("rect", { x: 0, y: 0, width: 100, height: 100, rx: RADIUS, ry: RADIUS, fill: "none", stroke: `url(#${id}H)`, "stroke-width": 44, "stroke-linejoin": "round", "stroke-linecap": "round", ...ns }, el("g", { transform: M, "clip-path": `url(#${id}clip)` }, inner));
    // halos exteriores, siguen las mismas zonas claras y oscuras
    rect(el("g", { filter: "url(#mgrb5)", opacity: 0.7 }, halo), { fill: "none", stroke: `url(#${id}H)`, "stroke-width": 18, ...ns });
    rect(el("g", { filter: "url(#mgrb4)", opacity: 1 }, halo), { fill: "none", stroke: `url(#${id}H)`, "stroke-width": 12, ...ns });

    // ---- line: the thin lines of the edges ----
    const edge = el("g", { mask: `url(#${id}mask)` }, layer("is-glow"));
    rect(el("g", { filter: "url(#mgrb2)", opacity: 1 }, edge), { fill: "none", stroke: `url(#${id}A)`, "stroke-width": 5, ...ns });
    rect(el("g", { filter: "url(#mgrb2)", opacity: 0.7 }, edge), { fill: "none", stroke: `url(#${id}C)`, "stroke-width": 4, ...ns });
    // canto inferior: segunda línea finita bajo los bordes frontales
    const lip = `M${L.x + 12} ${L.y + 5} L${F.x} ${F.y + 5} L${R.x - 12} ${R.y + 5}`;
    el("path", { d: lip, fill: "none", stroke: p.glow, "stroke-width": 4, opacity: 0.45, filter: "url(#mgrb2)", "stroke-linejoin": "round", "stroke-linecap": "round" }, edge);
    el("path", { d: lip, fill: "none", stroke: p.edge, "stroke-width": 1, opacity: 0.4, "stroke-linejoin": "round", "stroke-linecap": "round" }, edge);
    // líneas nítidas: color variable + reflejo
    rect(edge, { fill: "none", stroke: `url(#${id}A)`, "stroke-width": 2.6, ...ns });
    rect(edge, { fill: "none", stroke: `url(#${id}C)`, "stroke-width": 1.4, ...ns });
  });

  // zonas de clic: el rombo de cada placa; la de arriba se añade al final para quedar encima
  const hits = layerSvg("mgr-plates-svg");
  plates.forEach((p) => {
    const i = PLATES.indexOf(p);
    el("polygon", { class: "mgr-plate-hit", "data-hit": i, points: `${CX - HALF_W},${p.y} ${CX},${p.y - HALF_H} ${CX + HALF_W},${p.y} ${CX},${p.y + HALF_H}` }, hits);
  });

  // The light running along the edges. The lines follow it every frame; the halos, one plate per
  // frame in turns (`all`: every one of them now).
  let turn = 0;
  const draw = (t, all = false) => {
    turn = (turn + 1) % ANIM.length;
    ANIM.forEach((a, n) => {
      const w = (x) => ((x % 100) + 100) % 100, o = w(t * a.speed * a.dir + a.phase), o2 = w(-(t * a.speed * a.dir * 1.3 + a.phase));
      a.ga.setAttribute("gradientTransform", `translate(${o.toFixed(3)} ${o.toFixed(3)})`);
      a.gc.setAttribute("gradientTransform", `translate(${o2.toFixed(3)} 0)`);
      if (all || n === turn) a.gh.setAttribute("gradientTransform", `translate(${o.toFixed(3)} ${o.toFixed(3)})`);
    });
  };
  // Floating: the same 7 s, 6 units ease as the design's CSS keyframes (each plate 1.4 s apart),
  // applied in the frame loop (so it moves at the loop's pace, not at the screen's) as a transform
  // of the layers: the browser moves the pictures it already has, nothing is drawn again.
  let unit = 1; // screen px per unit of the 1000×1000 space
  const floatY = (t, i) => -3 * (1 - Math.cos(((t + i * 1.4) / 7) * Math.PI * 2));
  const float = (t) =>
    groups.forEach((list, i) => {
      const move = `translateY(${(floatY(t, i) * unit).toFixed(2)}px)`;
      list.forEach((s) => (s.style.transform = move));
    });
  const drawAll = (t, all) => {
    draw(t, all);
    float(t);
  };
  draw(0, true);
  float(0);
  // classes on every layer of the plate (no wrapper around them: it would isolate the screen blending)
  const lift = { active: -1, hover: -1, cur: PLATES.map(() => 0), last: 0 };
  const mark = (cls, idx) => {
    groups.forEach((list, i) => list.forEach((g) => g.classList.toggle(cls, i === idx)));
    if (cls === "is-active") lift.active = idx;
    if (cls === "is-hover") lift.hover = idx ?? -1;
  };
  const setUnit = (u) => {
    unit = u;
    stage.style.setProperty("--u", `${u.toFixed(4)}px`);
  };

  // Waves canvas: each wave is stroked into a glow layer (half resolution, it's blurred anyway) and
  // a core layer, clipped to its plate; each layer is then blurred ONCE (σ 4 and 0.9 like the
  // design's filters) onto the canvas, which blends with screen like the SVG group did.
  const hexA = (hex, a) => `rgba(${parseInt(hex.slice(1, 3), 16)},${parseInt(hex.slice(3, 5), 16)},${parseInt(hex.slice(5, 7), 16)},${a})`;
  WAVES.forEach((w) => w.stops.forEach((st) => (st.rgba = hexA(st.color, st.a))));
  const wavePts = (w, t) => {
    const N = 300, out = new Float32Array((N + 1) * 2);
    for (let j = 0; j <= N; j++) {
      const u = j / N, a = u < 0.5 ? w.L : w.F, b = u < 0.5 ? w.F : w.R, v = u < 0.5 ? u * 2 : (u - 0.5) * 2;
      const x = a.x + (b.x - a.x) * v, y = a.y + (b.y - a.y) * v;
      const span = 2 * HALF_H * (1 - Math.abs(2 * u - 1));
      const tt = t * w.speed;
      let n = vn(j * w.fine + tt + w.ph) * 0.5 + vn(j * w.fine * 2.6 - tt * 1.3 + w.ph * 3) * 0.35 * w.jag + vn(j * w.fine * 5.5 + tt * 0.8 + w.ph * 7) * 0.2 * w.jag;
      const bump = vn(j * 0.06 + tt * 0.4 + w.ph * 5);
      n += Math.pow(Math.max(0, bump - 0.6) / 0.4, 2) * w.spike;
      out[j * 2] = x;
      out[j * 2 + 1] = y - Math.min(span * 0.5, w.base + w.amp * n);
    }
    return out;
  };
  // spreadMethod="reflect" from L.x to L.x + 60% of the plate, moved by translate(t·drift)
  const waveGradient = (ctx, w, t) => {
    const P = (w.R.x - w.L.x) * 0.6;
    const off = (((t * w.drift) % (2 * P)) + 2 * P) % (2 * P);
    const x0 = w.L.x + off - 4 * P;
    const g = ctx.createLinearGradient(x0, 0, x0 + 8 * P, 0);
    for (let h = 0; h < 8; h++) {
      const list = h % 2 ? w.stops.slice().reverse() : w.stops;
      for (const st of list) g.addColorStop((h + (h % 2 ? 1 - st.o : st.o)) / 8, st.rgba);
    }
    return g;
  };
  // the chosen plate rises 12 units, the hovered one 6 (CSS transition 0.4 s on the SVG groups)
  const liftNow = () => {
    const now = performance.now(), dt = Math.min(0.1, (now - (lift.last || now)) / 1000);
    lift.last = now;
    lift.cur = lift.cur.map((c, i) => {
      const target = i === lift.active ? -12 : i === lift.hover ? -6 : 0;
      return c + (target - c) * (1 - Math.exp(-dt / 0.09));
    });
  };
  let glowC, coreC, glowX, coreX; // made if this painter is the one used
  const paintWaves = (ctx, t) => {
    const cw = ctx.canvas.width, ch = ctx.canvas.height;
    if (cw < 2) return;
    const sc = cw / W;
    if (!glowC) {
      glowC = document.createElement("canvas");
      coreC = document.createElement("canvas");
      glowX = glowC.getContext("2d");
      coreX = coreC.getContext("2d");
    }
    if (glowC.width !== Math.ceil(cw / 2)) { glowC.width = Math.ceil(cw / 2); glowC.height = Math.ceil(ch / 2); }
    if (coreC.width !== cw) { coreC.width = cw; coreC.height = ch; }
    liftNow();
    glowX.setTransform(1, 0, 0, 1, 0, 0);
    glowX.clearRect(0, 0, glowC.width, glowC.height);
    coreX.setTransform(1, 0, 0, 1, 0, 0);
    coreX.clearRect(0, 0, cw, ch);
    let plateNow = -1;
    for (const w of WAVES) {
      const oy = floatY(t, w.i) + lift.cur[w.i];
      if (w.i !== plateNow) {
        if (plateNow >= 0) {
          glowX.restore();
          coreX.restore();
        }
        plateNow = w.i;
        const clip = new Path2D(`M${w.L.x} ${w.L.y}L${w.B.x} ${w.B.y}L${w.R.x} ${w.R.y}L${w.F.x} ${w.F.y}Z`); // solo dentro de la placa
        glowX.save();
        glowX.setTransform(sc / 2, 0, 0, sc / 2, 0, (oy * sc) / 2);
        glowX.clip(clip);
        coreX.save();
        coreX.setTransform(sc, 0, 0, sc, 0, oy * sc);
        coreX.clip(clip);
      }
      const pts = wavePts(w, t), path = new Path2D();
      path.moveTo(pts[0], pts[1]);
      for (let j = 2; j < pts.length; j += 2) path.lineTo(pts[j], pts[j + 1]);
      const grad = waveGradient(coreX, w, t);
      glowX.strokeStyle = grad;
      glowX.globalAlpha = 0.6;
      glowX.lineWidth = w.k === 0 ? 5 : 4;
      glowX.lineCap = "round";
      glowX.stroke(path);
      coreX.strokeStyle = grad;
      coreX.globalAlpha = 0.55;
      coreX.lineWidth = w.k === 0 ? 2 : 1.6;
      coreX.lineCap = "round";
      coreX.lineJoin = "round";
      coreX.stroke(path);
    }
    if (plateNow >= 0) {
      glowX.restore();
      coreX.restore();
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, cw, ch);
    ctx.filter = `blur(${(4 * sc).toFixed(2)}px)`;
    ctx.drawImage(glowC, 0, 0, cw, ch);
    ctx.filter = `blur(${(0.9 * sc).toFixed(2)}px)`;
    ctx.drawImage(coreC, 0, 0);
    ctx.filter = "none";
  };

  // The same waves with WebGL: returns the function that paints them at t. As paths of a 2D canvas
  // the fine lines (20 of 300 segments, new every frame) cost the browser's graphics process more
  // than everything else in the section; as triangles they cost next to nothing. Each layer (glow
  // at half resolution, core) is drawn smoothed (4 samples per pixel), with every pixel painted
  // once per wave (the stencil: a ribbon overlaps itself at sharp corners), then blurred across
  // and, onto the canvas, down.
  const wavesGl = (gl) => {
    const N = 301, PER = N * 2, FLOATS = 5; // points of a wave, its vertices, numbers per vertex
    const verts = new Float32Array(WAVES.length * PER * FLOATS);
    WAVES.forEach((w) => (w.rgb = [1, 3, 5].map((at) => parseInt(w.stops[0].color.slice(at, at + 2), 16) / 255))); // one colour per wave: its stops only differ in opacity
    // how visible the wave is at x: what waveGradient paints there
    const alphaAt = (w, x, t) => {
      const P = (w.R.x - w.L.x) * 0.6;
      let q = ((((x - w.L.x - t * w.drift) / P) % 2) + 2) % 2;
      if (q > 1) q = 2 - q;
      const s = q * 9, k = Math.min(8, Math.floor(s));
      return w.stops[k].a + (w.stops[k + 1].a - w.stops[k].a) * (s - k);
    };
    const fill = (t) => {
      let o = 0;
      for (const w of WAVES) {
        const pts = wavePts(w, t);
        for (let j = 0; j < N; j++) {
          const x = pts[j * 2], y = pts[j * 2 + 1], a = j ? j - 1 : 1, b = j < N - 1 ? j + 1 : j - 1, back = j ? 1 : -1, on = j < N - 1 ? 1 : -1;
          // the line's direction before and after the point; across the corner between them, as
          // long as the corner needs (up to twice the width, at the sharpest ones)
          let ax = (x - pts[a * 2]) * back, ay = (y - pts[a * 2 + 1]) * back, bx = (pts[b * 2] - x) * on, by = (pts[b * 2 + 1] - y) * on;
          const al = Math.hypot(ax, ay) || 1, bl = Math.hypot(bx, by) || 1;
          ax /= al; ay /= al; bx /= bl; by /= bl;
          let tx = ax + bx, ty = ay + by;
          const tl = Math.hypot(tx, ty);
          if (tl < 1e-4) { tx = ax; ty = ay; } else { tx /= tl; ty /= tl; }
          const m = 1 / Math.max(0.5, tx * ax + ty * ay), sx = -ty * m, sy = tx * m, alpha = alphaAt(w, x, t);
          verts[o++] = x; verts[o++] = y; verts[o++] = sx; verts[o++] = sy; verts[o++] = alpha;
          verts[o++] = x; verts[o++] = y; verts[o++] = -sx; verts[o++] = -sy; verts[o++] = alpha;
        }
      }
    };

    const program = (vs, fs) => {
      const p = gl.createProgram();
      for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
        const sh = gl.createShader(type);
        gl.shaderSource(sh, src);
        gl.compileShader(sh);
        gl.attachShader(p, sh);
      }
      gl.linkProgram(p);
      if (!gl.getProgramParameter(p, gl.LINK_STATUS) && !gl.isContextLost()) throw new Error(gl.getProgramInfoLog(p) || "shader");
      const u = {};
      for (let k = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS) - 1; k >= 0; k--) {
        const name = gl.getActiveUniform(p, k).name;
        u[name] = gl.getUniformLocation(p, name);
      }
      return { p, u };
    };
    // What a layer is drawn on, w×h: `ms` (smoothed, with stencil), `flat` (that as a texture,
    // `sharp`) and `mid` (blurred across, the texture `soft`).
    const target = (w, h, sigma, opacity) => {
      const made = [];
      const tex = () => {
        const x = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, x);
        gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, w, h);
        for (const [k, v] of [[gl.TEXTURE_MIN_FILTER, gl.LINEAR], [gl.TEXTURE_MAG_FILTER, gl.LINEAR], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE]]) gl.texParameteri(gl.TEXTURE_2D, k, v);
        made.push(() => gl.deleteTexture(x));
        return x;
      };
      const buffer = (format) => {
        const x = gl.createRenderbuffer();
        gl.bindRenderbuffer(gl.RENDERBUFFER, x);
        gl.renderbufferStorageMultisample(gl.RENDERBUFFER, 4, format, w, h);
        made.push(() => gl.deleteRenderbuffer(x));
        return x;
      };
      const frame = (attach) => {
        const x = gl.createFramebuffer();
        gl.bindFramebuffer(gl.FRAMEBUFFER, x);
        attach();
        made.push(() => gl.deleteFramebuffer(x));
        return x;
      };
      const color = buffer(gl.RGBA8), stencil = buffer(gl.DEPTH24_STENCIL8), sharp = tex(), soft = tex();
      return {
        w, h, sharp, soft, opacity,
        sc: w / W,
        sigma: (sigma * w) / W, // in its own pixels
        ms: frame(() => {
          gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, color);
          gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_STENCIL_ATTACHMENT, gl.RENDERBUFFER, stencil);
        }),
        flat: frame(() => gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, sharp, 0)),
        mid: frame(() => gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, soft, 0)),
        free: () => made.forEach((f) => f()),
      };
    };

    let line, blur, vbo, lost = false, layers = null, size = "";
    const build = () => {
      line = program(WAVE_VS, WAVE_FS);
      blur = program(FULL_VS, BLUR_FS);
      vbo = gl.createBuffer();
      gl.bindVertexArray(gl.createVertexArray());
      gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
      gl.bufferData(gl.ARRAY_BUFFER, verts.byteLength, gl.DYNAMIC_DRAW);
      [["a_at", 2, 0], ["a_side", 2, 2], ["a_alpha", 1, 4]].forEach(([name, n, at]) => {
        const loc = gl.getAttribLocation(line.p, name);
        gl.enableVertexAttribArray(loc);
        gl.vertexAttribPointer(loc, n, gl.FLOAT, false, FLOATS * 4, at * 4);
      });
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA); // colours already multiplied by their opacity
      gl.stencilOp(gl.KEEP, gl.KEEP, gl.REPLACE);
      gl.clearColor(0, 0, 0, 0);
      layers = null;
      size = "";
    };
    build();
    // the graphics card was reset: everything made there is gone, and is made again when it is back
    gl.canvas.addEventListener("webglcontextlost", (e) => {
      e.preventDefault();
      lost = true;
    });
    gl.canvas.addEventListener("webglcontextrestored", () => {
      build();
      lost = false;
    });

    return (t) => {
      const cw = gl.drawingBufferWidth, ch = gl.drawingBufferHeight;
      if (lost || cw < 2) return;
      liftNow();
      if (size !== `${cw}x${ch}`) {
        size = `${cw}x${ch}`;
        layers?.forEach((T) => T.free());
        // the glow (half resolution, it's blurred anyway) and the core: σ 4 and 0.9 like the design's filters
        layers = [target(Math.ceil(cw / 2), Math.ceil(ch / 2), 4, 0.6), target(cw, ch, 0.9, 0.55)];
      }
      fill(t);
      gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, verts);
      layers.forEach((T, core) => {
        gl.bindFramebuffer(gl.FRAMEBUFFER, T.ms);
        gl.viewport(0, 0, T.w, T.h);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
        gl.enable(gl.BLEND);
        gl.enable(gl.STENCIL_TEST);
        gl.useProgram(line.p);
        gl.uniform2f(line.u.u_size, T.w, T.h);
        gl.uniform1f(line.u.u_sc, T.sc);
        gl.uniform1f(line.u.u_layer, T.opacity);
        WAVES.forEach((w, n) => {
          gl.stencilFunc(gl.NOTEQUAL, n + 1, 0xff);
          gl.uniform1f(line.u.u_half, (core ? (w.k === 0 ? 2 : 1.6) : w.k === 0 ? 5 : 4) / 2);
          gl.uniform1f(line.u.u_lift, floatY(t, w.i) + lift.cur[w.i]);
          gl.uniform2f(line.u.u_plate, CX, w.L.y);
          gl.uniform3fv(line.u.u_color, w.rgb);
          gl.drawArrays(gl.TRIANGLE_STRIP, n * PER, PER);
        });
        gl.disable(gl.STENCIL_TEST);
        gl.disable(gl.BLEND);
        gl.bindFramebuffer(gl.READ_FRAMEBUFFER, T.ms);
        gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, T.flat);
        gl.blitFramebuffer(0, 0, T.w, T.h, 0, 0, T.w, T.h, gl.COLOR_BUFFER_BIT, gl.NEAREST);
        gl.bindFramebuffer(gl.FRAMEBUFFER, T.mid);
        gl.useProgram(blur.p);
        gl.bindTexture(gl.TEXTURE_2D, T.sharp);
        gl.uniform2f(blur.u.u_step, 1 / T.w, 0);
        gl.uniform1f(blur.u.u_sigma, T.sigma);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      });
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(0, 0, cw, ch);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      for (const T of layers) {
        gl.bindTexture(gl.TEXTURE_2D, T.soft);
        gl.uniform2f(blur.u.u_step, 0, 1 / T.h);
        gl.uniform1f(blur.u.u_sigma, T.sigma);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      }
      gl.disable(gl.BLEND);
    };
  };
  return { svg: hits, draw: drawAll, mark, paintWaves, wavesGl, setUnit };
}

// Mounts the scene. `stage` is the square the plates are drawn in, `canvas` covers the whole
// section (the sea goes behind the side panels). Returns controls; nothing animates until start().
export function createPlatesScene(stage, canvas) {
  const plates = buildPlates(stage);
  const newWavesCanvas = () => {
    const c = Object.assign(document.createElement("canvas"), { className: "mgr-plates-waves" });
    c.setAttribute("aria-hidden", "true");
    stage.appendChild(c);
    return c;
  };
  // The waves are painted with WebGL; without it (or if it fails), as paths on a 2D canvas.
  let wavesCanvas = newWavesCanvas(), paintWaves;
  try {
    const gl = wavesCanvas.getContext("webgl2", { antialias: false, depth: false });
    if (!gl) throw new Error("WebGL 2 no disponible");
    paintWaves = plates.wavesGl(gl);
  } catch (err) {
    console.warn("Gestor: las ondas se dibujan sin WebGL:", err);
    wavesCanvas.remove();
    wavesCanvas = newWavesCanvas();
    const ctx = wavesCanvas.getContext("2d");
    paintWaves = (t) => plates.paintWaves(ctx, t);
  }
  const sizeWaves = () => {
    const r = stage.getBoundingClientRect(), pr = Math.min(devicePixelRatio || 1, 1.5);
    if (r.width > 0) plates.setUnit(r.width / W);
    const w = Math.max(2, Math.round(r.width * pr)), h = Math.max(2, Math.round(r.height * pr));
    if (wavesCanvas.width !== w || wavesCanvas.height !== h) {
      wavesCanvas.width = w;
      wavesCanvas.height = h;
    }
  };
  sizeWaves();
  let world = null;
  let running = false;
  let raf = 0, timer = 0;
  const request = () => {
    if (!raf) raf = requestAnimationFrame(frame);
  };
  const t0 = performance.now();
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // The island is built and drawn by a worker (manager-world.worker.js) on this canvas: from here
  // it is only told where the plates are and what time to draw.
  const worldLayout = () => {
    const b = canvas.getBoundingClientRect(), st = stage.getBoundingClientRect();
    return { width: b.width, height: b.height, stageLeft: st.left - b.left, stageTop: st.top - b.top, stageWidth: st.width, dpr: devicePixelRatio || 1 };
  };
  try {
    const offscreen = canvas.transferControlToOffscreen();
    const worker = new Worker(new URL("./manager-world.worker.js", import.meta.url));
    let ready = false, lastLayout = "";
    worker.onmessage = ({ data }) => {
      if (data.type === "ready") {
        ready = true;
        canvas.classList.add("is-ready");
        if (running) request();
      } else if (data.type === "failed") {
        console.warn("Gestor: no se pudo cargar el mundo 3D:", data.error);
        world = null;
        worker.terminate();
      }
    };
    const layout = worldLayout();
    lastLayout = JSON.stringify(layout);
    worker.postMessage({ type: "init", canvas: offscreen, consts: { BASE_Y, HALF_W, HALF_H }, layout }, [offscreen]);
    world = {
      resize() {
        const next = worldLayout(), key = JSON.stringify(next);
        if (key === lastLayout) return;
        lastLayout = key;
        worker.postMessage({ type: "layout", layout: next });
      },
      draw(t) {
        if (ready) worker.postMessage({ type: "draw", t });
      },
    };
  } catch (err) {
    console.warn("Gestor: no se pudo cargar el mundo 3D:", err);
  }

  // 30 fps: the motions are slow (light running along the edges, water, fire), and the launcher
  // shares the GPU with Wallpaper Engine, which stutters when this scene takes it every frame.
  const FRAME_MS = 1000 / 30;
  let lastFrame = 0;
  // The next frame is asked for when it is nearly due. Asked for at once, the page ran its whole
  // frame routine at every refresh of the screen (240 times per second at 240 Hz) to draw 30.
  // `early`: how long before it is due; a bit over one refresh of the screen (so a late timer
  // does not cost a frame), which is learnt from two frames in a row (`chained`: when the one
  // before asked for this one). Until then, and again when a frame comes late, enough for a
  // 60 Hz screen.
  let early = 12, chained = 0;
  function frame(now) {
    raf = 0;
    if (!running) return;
    if (chained) early = Math.min(12, Math.max(3, (now - chained) * 1.25));
    chained = 0;
    const since = now - lastFrame;
    if (since < FRAME_MS - 2) {
      chained = now;
      return request();
    }
    if (since > FRAME_MS + 10) early = 12;
    lastFrame = now;
    const t = (now - t0) / 1000;
    plates.draw(t, reduce); // without motion this is the only frame: every halo in its place
    paintWaves(t);
    if (world) world.draw(t);
    clearTimeout(timer);
    if (!reduce) timer = setTimeout(request, FRAME_MS - early);
  }

  const ro = new ResizeObserver(() => {
    sizeWaves();
    if (world) world.resize();
  });
  ro.observe(stage);
  ro.observe(canvas);

  return {
    svg: plates.svg,
    setActive(idx) {
      plates.mark("is-active", idx);
      plates.svg.classList.toggle("has-active", idx >= 0);
    },
    setHover(idx) {
      plates.mark("is-hover", idx);
    },
    start() {
      if (running) return;
      running = true;
      world?.resize();
      request();
    },
    stop() {
      running = false;
      cancelAnimationFrame(raf);
      clearTimeout(timer);
      raf = 0;
    },
    resize() {
      world?.resize();
    },
  };
}
