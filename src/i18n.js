// Interface language. Spanish is the language the launcher is written in; in English every text
// it shows is translated on the fly from src/i18n/en.json: the page as it loads, and whatever
// appears or changes later (windows, notices, errors), text and attributes alike. Switching
// language happens in place (no reload): each translated text remembers its Spanish, which comes
// back when switching to Spanish.
//   exact      "Spanish text" -> "English text"
//   templates  texts with dynamic parts, {0} {1}... ("hace {0} días" -> "{0} days ago")
// Numbers inside a text don't stop an exact match ("Descargando 45%" uses "Descargando 0%").
import EN from "./i18n/en.json";

const LANG_KEY = "ika_launcher_language";

let current = (() => {
  try {
    return localStorage.getItem(LANG_KEY) === "en" ? "en" : "es";
  } catch {
    return "es";
  }
})();

// The language the launcher is in now.
export const getLang = () => current;
export const getLocale = () => (current === "en" ? "en-US" : "es-ES");

const NUM = /\d+(?:[.,]\d+)?/g;
const collapse = (s) => s.replace(/\s+/g, " ").trim();
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

let exact = null; // Map: text -> English
let byNumbers = null; // Map: text with its numbers as "#" -> English with "#"
let templates = null; // [{re, en, order, chunk}]

function build() {
  if (exact) return;
  exact = new Map(Object.entries(EN.exact));
  byNumbers = new Map();
  for (const [es, en] of exact) {
    if (!/\d/.test(es)) continue;
    const esNums = es.match(NUM) || [];
    const enNums = en.match(NUM) || [];
    // only when the English keeps the same numbers in the same order
    if (esNums.length !== enNums.length || esNums.some((n, i) => n !== enNums[i])) continue;
    const key = es.replace(NUM, "#");
    if (!byNumbers.has(key)) byNumbers.set(key, en.replace(NUM, "#"));
  }
  templates = EN.templates.map(([es, en]) => {
    const parts = es.split(/\{\d+\}/);
    const order = (es.match(/\{(\d+)\}/g) || []).map((p) => Number(p.slice(1, -1)));
    const re = new RegExp(`^${parts.map(escapeRe).join("(.+?)")}$`, "s");
    const chunk = parts.reduce((a, b) => (b.length > a.length ? b : a), "");
    return { re, en, order, chunk, literal: parts.join("").length };
  });
  // the most specific first ("de {0} · {1}" before "de {0}")
  templates.sort((a, b) => b.literal - a.literal);
}

// The English of a Spanish text, or null when there is none (not Spanish, or not known).
function toEnglish(text) {
  if (!text || !/[A-Za-zÁÉÍÓÚáéíóúñÑ¿¡]/.test(text)) return null;
  build();
  const key = collapse(text);
  if (!key) return null;
  let out = exact.get(key);
  if (out === undefined && /\d/.test(key)) {
    const pattern = byNumbers.get(key.replace(NUM, "#"));
    if (pattern) {
      const nums = key.match(NUM);
      let i = 0;
      out = pattern.replace(/#/g, () => nums[i++] ?? "");
    }
  }
  if (out === undefined) {
    for (const t of templates) {
      if (t.chunk && !key.includes(t.chunk)) continue;
      const m = key.match(t.re);
      if (!m) continue;
      out = t.en.replace(/\{(\d+)\}/g, (_, n) => {
        const at = t.order.indexOf(Number(n));
        return at >= 0 ? m[at + 1] : "";
      });
      break;
    }
  }
  if (out === undefined) return null;
  // the spaces around the text stay as they were
  return text.match(/^\s*/)[0] + out + text.match(/\s*$/)[0];
}

// For code that builds a text itself: what it reads in the current language.
export function t(text) {
  return (current === "en" && toEnglish(text)) || text;
}

const ATTRS = ["title", "placeholder", "data-tip", "aria-label", "alt"];
const SKIP = new Set(["SCRIPT", "STYLE", "TEXTAREA", "NOSCRIPT"]);
// what each translated node said in Spanish, and what it was set to in English
const textOf = new WeakMap(); // text node -> {es, en}
const attrsOf = new WeakMap(); // element -> {name: {es, en}}

function translateText(node) {
  const value = node.nodeValue;
  const seen = textOf.get(node);
  if (seen && value === seen.en) return; // already ours
  const en = toEnglish(value);
  if (en === null || en === value) {
    textOf.delete(node);
    return;
  }
  textOf.set(node, { es: value, en });
  node.nodeValue = en;
}

function translateElement(el) {
  for (const name of ATTRS) {
    const value = el.getAttribute(name);
    if (!value) continue;
    let map = attrsOf.get(el);
    if (map?.[name] && value === map[name].en) continue;
    const en = toEnglish(value);
    if (en === null || en === value) continue;
    if (!map) attrsOf.set(el, (map = {}));
    map[name] = { es: value, en };
    el.setAttribute(name, en);
  }
}

// Every text node and element under `root` (text boxes: only their attributes, their own text is
// the player's).
function eachNode(root, onText, onElement) {
  if (root.nodeType === 3) return onText(root);
  if (root.nodeType !== 1) return;
  onElement(root);
  if (SKIP.has(root.tagName)) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
    acceptNode: (n) => {
      if (n.nodeType !== 1 || !SKIP.has(n.tagName)) return NodeFilter.FILTER_ACCEPT;
      onElement(n);
      return NodeFilter.FILTER_REJECT;
    },
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (n.nodeType === 3) onText(n);
    else onElement(n);
  }
}

const translateTree = (root) =>
  eachNode(root, translateText, (el) => {
    if (!SKIP.has(el.tagName) || el.tagName === "TEXTAREA") translateElement(el);
  });

// Back to Spanish: every node that still shows the English set here gets its Spanish back (a
// text the launcher changed since then is already what it should be).
function restoreTree(root) {
  eachNode(
    root,
    (node) => {
      const seen = textOf.get(node);
      if (seen && node.nodeValue === seen.en) node.nodeValue = seen.es;
      textOf.delete(node);
    },
    (el) => {
      const map = attrsOf.get(el);
      if (!map) return;
      for (const [name, { es, en }] of Object.entries(map)) if (el.getAttribute(name) === en) el.setAttribute(name, es);
      attrsOf.delete(el);
    }
  );
}

let observer = null;
let titleEs = null;

function startEnglish() {
  build();
  titleEs = document.title;
  translateTree(document.body);
  const title = toEnglish(document.title);
  if (title) document.title = title;
  observer = new MutationObserver((records) => {
    for (const r of records) {
      if (r.type === "characterData") translateText(r.target);
      else if (r.type === "attributes") translateElement(r.target);
      else for (const n of r.addedNodes) translateTree(n);
    }
  });
  observer.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
}

function stopEnglish() {
  observer?.disconnect();
  observer = null;
  restoreTree(document.body);
  if (titleEs !== null) document.title = titleEs;
}

// At startup: the page in the saved language.
export function initI18n() {
  document.documentElement.lang = current;
  if (current === "en") startEnglish();
}

// Changes the language in place, behind a short soft fade of the window (no reload). Code that
// draws texts itself (clock, dates) hears it through the "ika-language" event.
export function setLanguage(lang) {
  const next = lang === "en" ? "en" : "es";
  if (next === current) return;
  try {
    localStorage.setItem(LANG_KEY, next);
  } catch {}
  const root = document.body; // the window and whatever is open over it
  const swap = () => {
    current = next;
    document.documentElement.lang = next;
    if (next === "en") startEnglish();
    else stopEnglish();
    window.dispatchEvent(new CustomEvent("ika-language", { detail: next }));
  };
  if (!root.animate) return swap();
  const out = root.animate(
    [
      { opacity: 1, filter: "blur(0px) saturate(1)" },
      { opacity: 0.25, filter: "blur(6px) saturate(1.3)" },
    ],
    { duration: 170, easing: "cubic-bezier(0.4, 0, 1, 1)", fill: "forwards" }
  );
  out.finished.then(() => {
    swap();
    root
      .animate(
        [
          { opacity: 0.25, filter: "blur(6px) saturate(1.3)" },
          { opacity: 1, filter: "blur(0px) saturate(1)" },
        ],
        { duration: 300, easing: "cubic-bezier(0, 0, 0.2, 1)" }
      )
      .finished.then(() => out.cancel());
  });
}
