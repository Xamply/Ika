import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

test("dock and monitor resize events share a complete titlebar update", () => {
  const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const start = source.indexOf("function initCyberDockHairlines() {");
  const end = source.indexOf("\nfunction updateWallpaperTabUnderline", start);
  const code = source.slice(start, end).split("\n").filter((line) => !line.includes("import.meta")).join("\n");
  const frames = new Map(), listeners = new Map(), observers = new Map();
  const notch = { id: "notch" }, dock = { id: "dock" };
  let next = 0, viewport = 2560;
  const paintedWidths = [];
  const context = vm.createContext({
    console,
    document: {
      documentElement: {}, head: null,
      getElementById: () => notch,
      querySelector: () => dock,
    },
    window: { addEventListener: (type, callback) => listeners.set(type, callback) },
    requestAnimationFrame: (callback) => { frames.set(++next, callback); return next; },
    ResizeObserver: class {
      constructor(callback) { this.callback = callback; }
      observe(element) { observers.set(element.id, this.callback); }
    },
    MutationObserver: class { observe() {} },
    checkCyberDockHairlinesVars() {},
    updateTitlebarHairlines: () => paintedWidths.push(viewport),
    syncEdgePanelsHeight() {},
  });
  const flush = () => {
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach((callback) => callback());
  };
  vm.runInContext(`${code}\ninitCyberDockHairlines();`, context);
  flush();
  paintedWidths.length = 0;

  // The dock reports the temporary restored size before maximize finishes.
  viewport = 1280;
  observers.get("dock")();
  viewport = 2560;
  listeners.get("resize")();
  flush();
  assert.deepEqual(paintedWidths, [2560], "the dock must not swallow the final border update");

  // WebView zoom can settle without changing the notch's CSS dimensions.
  paintedWidths.length = 0;
  listeners.get("ika:monitor-changed")();
  observers.get("notch")();
  flush();
  assert.deepEqual(paintedWidths, [2560]);
  assert.equal(frames.size, 0);
});
