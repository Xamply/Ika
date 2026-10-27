// Themes, made when building (and served the same way by the dev server): every colour of the
// launcher's identity in style.css, index.html and the markup the code writes becomes a token,
// --ik-RRGGBB, whose value is that colour in Cyber Índigo (so Índigo looks exactly as written)
// and the colour src/theme/palette.js gives it in each theme (body.theme-<id>).
//
// The sources are written in Índigo as always; nothing in them names a token.
//   #a855f7                 -> rgb(var(--ik-a855f7))
//   rgba(168, 85, 247, 0.3) -> rgba(var(--ik-a855f7), 0.3)
//   #a855f780               -> rgba(var(--ik-a855f7), 0.502)
// In markup: stop-color / fill / stroke attributes become the same in a style (which wins over
// the attribute), and colours inside style="..." are rewritten too.
import fs from "node:fs";
import path from "node:path";
import { familyOf, themedRgb, tokenName, THEMES, DEFAULT_THEME, ACCENT, ROLES } from "./src/theme/palette.js";

// Rules that keep their colours whatever the theme: the theme cards show each theme's own.
const FIXED_SELECTOR = /theme-preview/;

const COLOR = /#([0-9a-fA-F]{6})([0-9a-fA-F]{2})?\b|rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*([\d.]+%?)\s*)?\)/g;

// The value with its identity colours as tokens; `seen` collects the colours tokenised.
const rgbOf = (hex6, r, g, b) => (hex6 ? [0, 2, 4].map((i) => parseInt(hex6.slice(i, i + 2), 16)) : [r, g, b].map(Number));

// A gradient is one family: when most of its colours are the accent (green / turquoise), its other
// identity colours are treated as accent too (a cyan stop in a turquoise border), so in a theme
// the whole gradient is the accent's colour and not a mix of families. null: each as it is.
function gradientFamily(text) {
  const fams = [];
  for (const m of text.matchAll(COLOR)) {
    const rgb = rgbOf(m[1], m[3], m[4], m[5]);
    if (rgb.every((v) => v <= 255)) fams.push(familyOf(rgb));
  }
  const ident = fams.filter(Boolean);
  const accent = ident.filter((f) => ACCENT.includes(f)).length;
  return ident.length > accent && accent * 2 >= ident.length ? "teal" : null;
}

const GRADIENT = /((?:repeating-)?(?:linear|radial|conic)-gradient)\(((?:[^()]|\((?:[^()]|\([^()]*\))*\))*)\)/g;

function tokenise(value, seen, as = undefined) {
  if (value.includes("data:")) return value; // colours inside an embedded image stay
  // each gradient of the value decides its own family; the rest goes as it is
  if (as === undefined && value.includes("gradient(")) {
    let out = "";
    let last = 0;
    for (const m of value.matchAll(GRADIENT)) {
      out += tokenise(value.slice(last, m.index), seen, null);
      out += `${m[1]}(${tokenise(m[2], seen, gradientFamily(m[2]))})`;
      last = m.index + m[0].length;
    }
    return out + tokenise(value.slice(last), seen, null);
  }
  return value.replace(COLOR, (match, hex6, hexAlpha, r, g, b, alpha) => {
    const rgb = rgbOf(hex6, r, g, b);
    if (rgb.some((v) => v > 255)) return match;
    const fam = familyOf(rgb);
    if (!fam) return match;
    const forced = as && !ACCENT.includes(fam) ? as : null;
    seen.add(forced ? `${rgb.join(",")}|${forced}` : rgb.join(","));
    const name = tokenName(rgb, forced);
    if (hexAlpha) return `rgba(var(${name}), ${+(parseInt(hexAlpha, 16) / 255).toFixed(3)})`;
    if (alpha !== undefined) return `rgba(var(${name}), ${alpha})`;
    return `rgb(var(${name}))`;
  });
}

// CSS: the declarations of every rule (but the fixed ones) and of keyframes.
function tokeniseCss(css, seen) {
  let out = "";
  let i = 0;
  // A small scanner rather than a full parser: it keeps the file byte for byte except the colour
  // values, and knows which selector each declaration block belongs to.
  const stack = [];
  let last = 0;
  while (i < css.length) {
    const ch = css[i];
    if (ch === "/" && css[i + 1] === "*") {
      const end = css.indexOf("*/", i + 2);
      i = end < 0 ? css.length : end + 2;
      continue;
    }
    if (ch === '"' || ch === "'") {
      const end = css.indexOf(ch, i + 1);
      i = end < 0 ? css.length : end + 1;
      continue;
    }
    if (ch === "{") {
      const selector = css.slice(css.lastIndexOf("}", i) + 1, i).replace(/\/\*[\s\S]*?\*\//g, "").trim();
      // @property: its initial-value must be a plain colour (the uses of the property follow the
      // theme anyway)
      const fixed = FIXED_SELECTOR.test(selector) || /^@property\b/.test(selector) || stack.some((s) => s.fixed);
      // the text up to here is selector / at-rule prelude: kept as is
      out += css.slice(last, i + 1);
      last = i + 1;
      stack.push({ fixed });
      i++;
      continue;
    }
    if (ch === "}" || ch === ";") {
      const top = stack[stack.length - 1];
      const chunk = css.slice(last, i + 1);
      out += top && !top.fixed && chunk.includes(":") ? tokenise(chunk, seen) : chunk;
      last = i + 1;
      if (ch === "}") stack.pop();
      i++;
      continue;
    }
    i++;
  }
  return out + css.slice(last);
}

// Markup (index.html, and the HTML the code writes in template strings).
function tokeniseMarkup(text, seen) {
  // the stops of an SVG gradient share its family
  text = text.replace(/<(linearGradient|radialGradient)\b[\s\S]*?<\/\1>/g, (block) => {
    const fam = gradientFamily([...block.matchAll(/stop-color=(["'])([^"']+)\1/g)].map((m) => m[2]).join(" "));
    return tokeniseTags(block, seen, fam);
  });
  // style="..." contents
  text = text.replace(/\bstyle=(["'])([^"']*)\1/g, (m, q, v) => (v.includes("var(--ik-") ? m : `style=${q}${tokenise(v, seen)}${q}`));
  return tokeniseTags(text, seen, undefined);
}

// presentation attributes: the same colour as a style, which wins over the attribute
function tokeniseTags(text, seen, as) {
  return text.replace(/<(stop|path|rect|circle|ellipse|line|polyline|polygon|g|text|use|svg)\b([^<>]*?)(\/?)>/g, (tag, name, attrs, close) => {
    if (/style=(["'])[^"']*var\(--ik-/.test(attrs)) return tag;
    const styles = [];
    for (const m of attrs.matchAll(/\b(stop-color|fill|stroke)=(["'])([^"']+)\2/g)) {
      const v = tokenise(m[3], seen, as === undefined ? undefined : as);
      if (v !== m[3]) styles.push(`${m[1]}: ${v}`);
    }
    if (!styles.length) return tag;
    if (/\bstyle=(["'])/.test(attrs)) attrs = attrs.replace(/\bstyle=(["'])([^"']*)\1/, (m, q, v) => `style=${q}${v.replace(/;?\s*$/, "; ")}${styles.join("; ")}${q}`);
    else attrs = `${attrs} style="${styles.join("; ")}"`;
    return `<${name}${attrs}${close}>`;
  });
}

// The tokens: their Índigo values and, per theme, the ones that change.
function tokenCss(all) {
  const list = [...all]
    .map((k) => {
      const [c, as] = k.split("|");
      return { rgb: c.split(",").map(Number), as: as || null };
    })
    .sort((a, b) => tokenName(a.rgb, a.as).localeCompare(tokenName(b.rgb, b.as)));
  const decl = (t, value) => `  ${tokenName(t.rgb, t.as)}: ${value.join(", ")};`;
  let out = `\n/* ---- Theme tokens (vite-plugin-ika-themes.js, src/theme/palette.js) ---- */\n:root {\n${list.map((t) => decl(t, t.rgb)).join("\n")}\n}\n`;
  for (const id of Object.keys(THEMES)) {
    if (id === DEFAULT_THEME) continue;
    const lines = list.map((t) => [t, themedRgb(t.rgb, id, t.as)]).filter(([t, b]) => t.rgb.join() !== b.join()).map(([t, b]) => decl(t, b));
    out += `body.theme-${id} {\n${lines.join("\n")}\n}\n`;
    // roles: the same tokens again, on those elements only, in the role's hue
    for (const role of Object.keys(THEMES[id].roles || {})) {
      const sel = (ROLES[role] || []).map((x) => `body.theme-${id} ${x}`).join(",\n");
      const rl = list.filter((t) => !t.as && !ACCENT.includes(familyOf(t.rgb))).map((t) => decl(t, themedRgb(t.rgb, id, null, role)));
      if (sel && rl.length) out += `${sel} {\n${rl.join("\n")}\n}\n`;
    }
  }
  return out;
}

export default function ikaThemes() {
  let root = process.cwd();
  // Every colour tokenised anywhere gets its token defined, whatever is transformed first.
  const markupSeen = new Set();
  const scanSources = () => {
    const seen = new Set();
    tokeniseMarkup(fs.readFileSync(path.join(root, "index.html"), "utf8"), seen);
    for (const f of fs.readdirSync(path.join(root, "src"))) if (f.endsWith(".js")) tokeniseMarkup(fs.readFileSync(path.join(root, "src", f), "utf8"), seen);
    return seen;
  };
  return {
    name: "ika-themes",
    enforce: "pre",
    configResolved(config) {
      root = config.root;
    },
    // the palette is read when the plugin loads: a change to it restarts the dev server
    configureServer(server) {
      const palette = path.join(root, "src", "theme", "palette.js");
      server.watcher.add(palette);
      server.watcher.on("change", (file) => {
        if (path.resolve(file) === path.resolve(palette)) server.restart();
      });
    },
    transform(code, id) {
      const file = id.split("?")[0];
      if (file.endsWith("/src/style.css")) {
        const seen = new Set();
        const css = tokeniseCss(code, seen);
        for (const k of scanSources()) seen.add(k);
        return { code: css + tokenCss(seen), map: null };
      }
      if (file.includes("/src/") && file.endsWith(".js") && !file.includes("/src/theme/")) {
        const out = tokeniseMarkup(code, markupSeen);
        return out === code ? null : { code: out, map: null };
      }
      return null;
    },
    transformIndexHtml: {
      order: "pre",
      handler(html) {
        // a page with its own <style> (the console window) gets its colours and tokens there;
        // index.html takes its tokens from style.css
        const own = new Set();
        html = html.replace(/(<style\b[^>]*>)([\s\S]*?)(<\/style>)/g, (m, open, css, close) => open + tokeniseCss(css, own) + close);
        html = tokeniseMarkup(html, html.includes("/src/style.css") ? markupSeen : own);
        if (own.size && !html.includes("/src/style.css")) html = html.replace("</head>", `<style>${tokenCss(own)}</style>\n</head>`);
        return html;
      },
    },
  };
}
