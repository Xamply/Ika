// Ika's themes. The launcher is drawn in one palette, Cyber Índigo: every colour of its identity
// (violets, indigos, sky blues, the turquoise, the fuchsias and the night-blue backgrounds) is
// written as is in style.css and index.html. A theme doesn't restyle anything: it says which hue
// each of those families takes, and every colour of the family follows, keeping its perceived
// lightness and its vividness (OKLCH), so a theme keeps the contrast and the hierarchy the
// launcher was designed with.
//
// Índigo is built on a trio: violet for everything, the mint green that is almost its opposite
// (complementary, so each makes the other stand out) and the neon pink beside it (analogous, it
// warms it without fighting it); sky blue against pink tells Comparador's two sides apart. Every theme keeps those same relations with its own trio.
//
// Red (errors), orange, amber (warnings) and the greys stay as they are in every theme.
//
// Used by the build (vite-plugin-ika-themes.js turns every identity colour into a token and writes
// each theme's value of it) and by the code that draws in canvas / WebGL (themeColor()).

// ---------------------------------------------------------------------------------------------
// Families of the identity, by hue (OKLCH degrees) of the Índigo colours
// ---------------------------------------------------------------------------------------------
// base: the hue the family is centred on in Índigo (where a theme's hue lands).
export const FAMILIES = {
  sky: { base: 238, from: 205, to: 250 }, // #38bdf8 #0ea5e9 #00d2ff: Comparador's left side
  indigo: { base: 272, from: 250, to: 282 }, // #6366f1 #818cf8 #4b15f2
  violet: { base: 300, from: 282, to: 318 }, // #a855f7 #c084fc #7c3aed: the main colour
  fuchsia: { base: 335, from: 318, to: 10 }, // #d946ef #e879f9 #ff2a85 (neon pink sits at 2): the contrast
  teal: { base: 180, from: 166, to: 205 }, // #00f5d4 #06f3e8
  green: { base: 155, from: 135, to: 166 }, // #2df49d #00ff88 #34d399: the accent
};
// The dark backgrounds (night blue / night violet): any identity hue, very dark.
const SURFACE_MAX_L = 0.3;
// Below this chroma a colour is a grey (slate texts and borders), not identity.
const MIN_CHROMA = 0.045;

// ---------------------------------------------------------------------------------------------
// Themes. Índigo's sky blue (238), indigo (272), violet (300) and pink (335) lie in order around
// the colour wheel: Comparador's left side is one end, its right side the other, and the orb in
// between is where they meet, which is why they blend there. A theme turns that whole arc:
//   hue     where the violet (the main colour) goes
//   arc     1 = the arc as in Índigo, -1 = mirrored (what was on the left goes to the right),
//           other values widen or narrow it
//   accent  where the mint green goes (the turquoise follows it), Índigo's: 155, almost opposite
//           to its violet
//   stops   instead of hue/arc: the hue each point of the arc takes, {sky, indigo, violet,
//           fuchsia} (left side, Play button, main colour, right side), for an arc that isn't
//           Índigo's turned (Fuego: yellow, gold, orange, red)
//   heat    hue by lightness, like fire: [[lightness, hue], ...] (Fuego: dark embers red, then
//           orange, the brightest yellow); the arc still tells the sides apart a little
//   roles   a hue for some kinds of element only, whatever the rest does: {pill, icon} (Fuego:
//           yellow selection pills and icons). See ROLES.
//   surface the hue of the dark backgrounds and how much colour they keep
//   lift    how far vivid colours go towards the lightest their hue gets; by default it depends
//           on the hue (a real yellow only exists light, darker it turns mustard; reds stay deep)
// ---------------------------------------------------------------------------------------------
export const THEMES = {
  indigo: { name: "Cyber Índigo", tag: "Predeterminado" },
  // the reds: fire yellow as the accent; Coral and Rubí a narrow arc around red (rose-red on the
  // left, gold on the right) so their Play button is crimson
  "crimson-coral": { name: "Crimson Coral", tag: "Destello Coral Neón", hue: 42, arc: 0.6, accent: 95, surface: { h: 22, c: 0.9 } },
  // Fuego: magenta on the left, a crimson Play button, gold on the right (a red sunset), cyan as
  // the accent
  "crimson-amber": { name: "Crimson Fuego", tag: "Destello Ámbar / Magma", hue: 50, arc: 1, accent: 205, surface: { h: 45, c: 0.75 }, roles: { pill: 96, icon: 96 } },
  "crimson-ruby": { name: "Crimson Rubí", tag: "Destello Rubí Profundo", hue: 34, arc: 0.5, accent: 85, surface: { h: 15, c: 1.1 } },
  // violet on the left, peach on the right, mint as the accent
  pink: { name: "Neon Sakura", tag: "Rosa Neón", hue: 350, arc: 1.4, accent: 165, surface: { h: 340, c: 0.85 } },
  // cyan on the left, lime on the right, pink (its opposite) as the accent
  green: { name: "Matrix Esmeralda", tag: "Verde Neón", hue: 150, arc: -1, accent: 345, surface: { h: 160, c: 0.8 } },
  // Celeste Neón: ultramarine and azure (its colours taken from a cobalt starry wallpaper by "Tema
  // según el fondo"), night-blue backgrounds, and hot pink where Índigo has green: almost the
  // blue's opposite, so it stands out clearly
  cyan: { name: "Celeste Neón", tag: "Hielo Eléctrico", hue: 267.5, arc: 0.45, accent: 338, surface: { h: 267.5, c: 1.15 } },
  // Solar Magma: fire from yellow to red: dark embers red, a golden Play button, orange as its
  // main colour, red on the right, the brightest sparks gold, fire yellow as the accent
  orange: {
    name: "Solar Magma",
    tag: "Atardecer",
    stops: { sky: 102, indigo: 88, violet: 58, fuchsia: 30 },
    heat: [[0.3, 24], [0.5, 38], [0.66, 58], [0.8, 84], [0.92, 100]],
    accent: 98,
    surface: { h: 34, c: 0.95 },
  },
  // blue on the left, green on the right (the northern lights), violet as the accent
  aurora: { name: "Aurora Boreal", tag: "Verde y Violeta", hue: 165, arc: -1, accent: 305, surface: { h: 230, c: 1.1 } },
};
// The elements of each role (THEMES[].roles): the theme's tokens are redefined on them, so all
// their shades, glows and gradients take the role's hue.
export const ROLES = {
  // the selection pills: main menu, Inicio's instance filter, Wallpapers' pages, Grupos' tabs and
  // Abierto/Selecto
  pill: [".view-switcher-glider", ".inst-pill-glider", ".steam-page-btn.active", ".seg-thumb"],
  // the icons of the instance's details bar (mods, RAM, loader...)
  icon: [".instance-specs-bar .spec-icon"],
};

export const DEFAULT_THEME = "indigo";
export const THEME_IDS = Object.keys(THEMES);

// ---------------------------------------------------------------------------------------------
// Colour maths: sRGB <-> OKLab / OKLCH
// ---------------------------------------------------------------------------------------------
const toLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const toGamma = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

export function rgbToOklch([r, g, b]) {
  const [lr, lg, lb] = [r, g, b].map((v) => toLinear(v / 255));
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  const C = Math.hypot(A, B);
  const H = ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360;
  return [L, C, H];
}

function oklchToLinear([L, C, H]) {
  const a = C * Math.cos((H * Math.PI) / 180);
  const b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

// OKLCH -> sRGB 0..255, lowering the chroma until it fits in sRGB (the hue and lightness stay).
export function oklchToRgb([L, C, H]) {
  const inGamut = (lin) => lin.every((v) => v >= -0.0005 && v <= 1.0005);
  let lin = oklchToLinear([L, C, H]);
  if (!inGamut(lin)) {
    let lo = 0;
    let hi = C;
    for (let i = 0; i < 24; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToLinear([L, mid, H]))) lo = mid;
      else hi = mid;
    }
    lin = oklchToLinear([L, lo, H]);
  }
  return lin.map((v) => Math.round(Math.min(1, Math.max(0, toGamma(Math.min(1, Math.max(0, v))))) * 255));
}

// ---------------------------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------------------------
const inRange = (h, { from, to }) => (from <= to ? h >= from && h < to : h >= from || h < to);

// The family of an Índigo colour, or null when it isn't part of the identity (greys, white,
// black, and the colours that mean something).
export function familyOf(rgb) {
  const [L, C, H] = rgbToOklch(rgb);
  if (C < MIN_CHROMA && !(L < SURFACE_MAX_L && C >= 0.02 && H > 230 && H < 330)) return null;
  // dark greens (the backgrounds of success tags) stay with their family
  for (const [name, fam] of Object.entries(FAMILIES)) if (inRange(H, fam)) return L < SURFACE_MAX_L && name !== "green" ? "surface" : name;
  return null;
}

// The most vivid colour of a hue that sRGB can show: its lightness and chroma (cached per degree).
const cusps = new Map();
function cusp(h) {
  const key = Math.round(h) % 360;
  let c = cusps.get(key);
  if (c) return c;
  let best = [0.5, 0];
  for (let L = 0.2; L <= 0.99; L += 0.01) {
    let lo = 0;
    let hi = 0.4;
    for (let i = 0; i < 18; i++) {
      const mid = (lo + hi) / 2;
      if (oklchToLinear([L, mid, key]).every((v) => v >= 0 && v <= 1)) lo = mid;
      else hi = mid;
    }
    if (lo > best[1]) best = [L, lo];
  }
  cusps.set(key, (c = best));
  return c;
}

// A colour moved to another hue keeping how vivid it is for its hue: a vivid violet becomes a vivid
// orange (with the same lightness it would be a dull brown, violets being dark by nature), but
// only part of the way towards the lightest that hue gets, so the shading and highlights drawn
// over it keep their contrast. Muted and dark colours keep their lightness.
const smooth = (a, b, x) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

// The most chroma a colour of lightness L and hue h can have in sRGB (cached).
const maxC = new Map();
function maxChroma(L, h) {
  const key = `${Math.round(L * 200)}|${Math.round(h) % 360}`;
  let c = maxC.get(key);
  if (c !== undefined) return c;
  let lo = 0;
  let hi = 0.4;
  const hh = Math.round(h) % 360;
  const LL = Math.round(L * 200) / 200;
  for (let i = 0; i < 16; i++) {
    const mid = (lo + hi) / 2;
    if (oklchToLinear([LL, mid, hh]).every((v) => v >= -0.0005 && v <= 1.0005)) lo = mid;
    else hi = mid;
  }
  maxC.set(key, lo);
  return lo;
}

function moveHue([L, C, H], h2, cMul = 1, liftAt = null) {
  const [Ls, Cs] = cusp(H);
  const [Lt] = cusp(h2);
  // hues whose vivid colours are light (yellow, cyan, green) go further towards it
  const lift = liftAt ?? 0.3 + 0.55 * smooth(0.74, 0.95, Lt);
  const vivid = Math.min(1, C / Cs);
  const Lrel = L <= Ls ? (L / Ls) * Lt : Lt + ((L - Ls) / (1 - Ls)) * (1 - Lt);
  // only part of the way: a body as light as its hue allows leaves no room for the highlights and
  // the shading on top of it (the Play button looked flat in the light hues)
  // (never past the lightness its place on the hue would give it)
  const L2 = Math.min(Math.max(L, Lrel), L + lift * vivid * vivid * (Lrel - L));
  // as saturated as it was for its lightness: a pale lavender that is nearly as vivid as a pale
  // colour can be stays nearly that vivid (a pale gold, not beige)
  const sat = Math.min(1, C / Math.max(1e-4, maxChroma(L, H)));
  return oklchToRgb([L2, sat * maxChroma(L2, h2) * cMul, (h2 + 360) % 360]);
}

const ARC = ["sky", "indigo", "violet", "fuchsia"];
const VIOLET = FAMILIES.violet.base;
const signed = (d) => ((d + 540) % 360) - 180;
const KNOTS = ["sky", "indigo", "violet", "fuchsia"].map((f) => FAMILIES[f].base); // 238 272 300 335

// The hue an Índigo arc hue takes in a theme: Índigo's arc turned (hue, arc), or drawn through
// the theme's own stops (each family's centre lands on its stop, in between it is interpolated,
// past the ends it keeps the last stretch's direction).
function arcHue(theme, H) {
  const x = VIOLET + signed(H - VIOLET);
  if (!theme.stops) return theme.hue + (x - VIOLET) * (theme.arc ?? 1);
  // the stops as a continuous run of hues (no jump across 0/360)
  const ys = [];
  ARC.forEach((f, i) => ys.push(i ? ys[i - 1] + signed(theme.stops[f] - ys[i - 1]) : theme.stops[f]));
  let i = 0;
  while (i < KNOTS.length - 2 && x > KNOTS[i + 1]) i++;
  const t = (x - KNOTS[i]) / (KNOTS[i + 1] - KNOTS[i]);
  return ys[i] + (ys[i + 1] - ys[i]) * t;
}

// Fire: the hue follows the lightness (embers red, flames orange, the brightest yellow).
function heatHue(theme, L, H) {
  const k = theme.heat;
  let h = k[0][1];
  if (L >= k[k.length - 1][0]) h = k[k.length - 1][1];
  else
    for (let i = 0; i < k.length - 1; i++)
      if (L >= k[i][0] && L < k[i + 1][0]) {
        const t = smooth(k[i][0], k[i + 1][0], L);
        h = k[i][1] + (k[i + 1][1] - k[i][1]) * t;
      }
  // the fire rules the embers and the brightest sparks; the mid tones keep the arc (Fuego's Play
  // button stays golden, its main colour orange, its right side red)
  const arc = arcHue(theme, H);
  const w = Math.min(1, 1 - smooth(0.34, 0.5, L) + smooth(0.8, 0.93, L));
  return arc + (h - arc) * w;
}

const toLab = (rgb) => {
  const [L, C, H] = rgbToOklch(rgb);
  return [L, C * Math.cos((H * Math.PI) / 180), C * Math.sin((H * Math.PI) / 180)];
};
const fromLab = ([L, a, b]) => oklchToRgb([L, Math.hypot(a, b), ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360]);
const mixLab = (x, y, t) => {
  const [a, b] = [toLab(x), toLab(y)];
  return fromLab(a.map((v, i) => v + (b[i] - v) * t));
};

// A dark background colour in the theme: its night blue takes the theme's background hue.
function surfaceRgb(theme, [L, C, H]) {
  const sf = theme.surface || {};
  return oklchToRgb([L, C * (sf.c ?? 1), (sf.h ?? H) + 0.25 * signed(H - 285)]);
}

// The colour an Índigo colour takes in `theme` (the same colour for Índigo, or when it isn't
// part of the identity). `as`: the family to treat it as (a stop of a gradient that belongs to
// another family).
export function themedRgb(rgb, themeId, as = null, role = null) {
  let theme = THEMES[themeId];
  // a role: the whole arc in its one hue
  if (role && theme?.roles?.[role] !== undefined) {
    const h = theme.roles[role];
    // as bright as the hue gets: a role is meant to stand out (a yellow at half light is mustard)
    theme = { ...theme, stops: { sky: h, indigo: h, violet: h, fuchsia: h }, heat: null, lift: 1.3, c: 1.1 };
  }
  const fam = as || familyOf(rgb);
  if (!theme || (theme.hue === undefined && !theme.stops) || !fam) return rgb;
  const [L, C, H] = rgbToOklch(rgb);
  // green and turquoise (and any stop of a green gradient): one family, the accent, keeping only
  // a little of their variety so an accent element is one colour, not several
  if (ACCENT.includes(fam)) return moveHue([L, C, H], theme.accent + 0.25 * signed(H - FAMILIES.green.base), theme.c, theme.lift);
  const surface = surfaceRgb(theme, [L, C, H]);
  if (fam === "surface" && (H < 205 || H > 360)) return surface;
  // the arc; the darkest of it fades into the theme's backgrounds without a seam, so a gradient
  // from night blue to neon stays smooth
  const vivid = moveHue([L, C, H], theme.heat ? heatHue(theme, L, H) : arcHue(theme, H), theme.c, theme.lift);
  const t = smooth(0.14, 0.42, L);
  return t >= 1 ? vivid : t <= 0 ? surface : mixLab(surface, vivid, t);
}

export const hex = (rgb) => `#${rgb.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
export const ACCENT = ["green", "teal"];
export const tokenName = (rgb, as = null) => `--ik-${hex(rgb).slice(1)}${as ? `-${as}` : ""}`;
