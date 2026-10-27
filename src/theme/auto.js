// "Tema según el fondo" (Aspecto visual): a theme made from the wallpaper's own colours, by the
// same rules as the others (src/theme/palette.js). Its main colour is the wallpaper's dominant
// hue; a second hue, when the wallpaper has one, becomes the right side of the arc, and the accent
// when it contrasts with the first; a one-colour wallpaper gets a quiet, analogous theme.
import { rgbToOklch, themedRgb, THEMES } from "./palette.js";

export const AUTO_ID = "auto";
const signed = (d) => ((d + 540) % 360) - 180;

// The theme for an RGBA pixel buffer (any size), or null when the picture has no colour to speak
// of (black and white, greys).
export function themeFromPixels(px) {
  const BINS = 72;
  const hist = new Float64Array(BINS);
  let total = 0;
  for (let i = 0; i < px.length; i += 4) {
    const [L, C, H] = rgbToOklch([px[i], px[i + 1], px[i + 2]]);
    if (L < 0.12 || L > 0.97 || C < 0.03) continue;
    const w = Math.pow(C, 1.5) * (0.4 + L);
    hist[Math.floor(H / (360 / BINS)) % BINS] += w;
    total += w;
  }
  if (total < 1e-3) return null;
  // smoothed around the wheel
  const smooth = hist.map((_, i) => {
    let s = 0;
    for (let k = -3; k <= 3; k++) s += hist[(i + k + BINS) % BINS] * (4 - Math.abs(k));
    return s;
  });
  const peakOf = (skip) => {
    let best = -1;
    for (let i = 0; i < BINS; i++) {
      const h = i * (360 / BINS);
      if (skip !== null && Math.abs(signed(h - skip)) < 50) continue;
      if (best < 0 || smooth[i] > smooth[best]) best = i;
    }
    return best;
  };
  const p1 = peakOf(null);
  const h1 = p1 * (360 / BINS) + 2.5;
  const p2 = peakOf(h1);
  const h2 = p2 >= 0 && smooth[p2] > smooth[p1] * 0.25 ? p2 * (360 / BINS) + 2.5 : null;
  const theme = { name: "Según el fondo", hue: h1, arc: 0.45, accent: h1 - 33, surface: { h: h1, c: 1.15 }, fromWallpaper: [h1, h2] };
  if (h2 !== null) {
    const d = signed(h2 - h1);
    // the second colour on the right side of the arc (pink sits 35 degrees from the violet)
    theme.arc = Math.max(-1.6, Math.min(1.6, d / 35));
    if (Math.abs(theme.arc) < 0.4) theme.arc = Math.sign(theme.arc || 1) * 0.4;
    // and the accent, when it is far enough to stand out
    if (Math.abs(d) > 90) theme.accent = h2;
  }
  return theme;
}

// The CSS of a theme made at run time: every token the stylesheet defines in :root, in it.
export function themeCss(id) {
  const tokens = [];
  for (const sheet of document.styleSheets) {
    let rules;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    for (const rule of rules) {
      if (rule.selectorText !== ":root") continue;
      for (let i = 0; i < rule.style.length; i++) {
        const name = rule.style[i];
        if (!name.startsWith("--ik-")) continue;
        const rgb = rule.style.getPropertyValue(name).split(",").map((v) => Number(v.trim()));
        if (rgb.length === 3 && rgb.every((v) => Number.isFinite(v))) tokens.push([name, rgb, name.split("-").length > 4 ? name.split("-").pop() : null]);
      }
    }
  }
  if (!THEMES[id]) return "";
  const lines = tokens.map(([name, rgb, as]) => `  ${name}: ${themedRgb(rgb, id, as).join(", ")};`);
  return `body.theme-${id} {\n${lines.join("\n")}\n}\n`;
}

// Puts the theme `def` in place as THEMES.auto and its CSS in the page.
export function installAutoTheme(def) {
  THEMES[AUTO_ID] = def;
  let style = document.getElementById("ika-auto-theme");
  if (!style) {
    style = document.createElement("style");
    style.id = "ika-auto-theme";
    document.head.appendChild(style);
  }
  style.textContent = themeCss(AUTO_ID);
}

// The wallpaper as pixels: Wallpaper Engine's live picture, or a frame of the video wallpaper.
export async function sampleWallpaper(invoke) {
  if (document.body.classList.contains("has-we-active")) {
    try {
      // the capture sleeps when nobody asks for it, and its first frame after that can still be
      // the previous wallpaper: one request to wake it, the frame of the next
      await invoke("capture_we_mini_thumb").catch(() => null);
      await new Promise((r) => setTimeout(r, 300));
      const buf = await invoke("capture_we_mini_thumb");
      const bytes = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf || []);
      if (bytes.length > 8) {
        const view = new DataView(bytes.buffer, bytes.byteOffset, 8);
        const w = view.getUint32(0, true);
        const h = view.getUint32(4, true);
        if (bytes.length >= 8 + w * h * 4) {
          // already RGBA (the big capture, capture_we_snapshot, is BGRA)
          return new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 8, w * h * 4);
        }
      }
    } catch {}
  }
  const video = document.getElementById("home-bg-video");
  if (video && video.readyState >= 2 && video.videoWidth) {
    try {
      const c = document.createElement("canvas");
      c.width = 96;
      c.height = 54;
      const x = c.getContext("2d", { willReadFrequently: true });
      x.drawImage(video, 0, 0, c.width, c.height);
      return x.getImageData(0, 0, c.width, c.height).data;
    } catch {}
  }
  return null;
}
