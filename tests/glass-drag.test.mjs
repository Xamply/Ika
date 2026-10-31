import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const code = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");

test("glass tracks latest drag position each display frame, including scaled panels and origin", () => {
  const events = {}, frames = new Map();
  let frameId = 0, writes = 0, x = 160, y = 80;
  const properties = new Map();
  const canvas = { style: { width: "", height: "" } };
  const layer = {
    checkVisibility: () => true, offsetWidth: 500, offsetHeight: 250,
    getBoundingClientRect: () => ({ left: x, top: y, width: 400, height: 200 }),
    querySelector: () => canvas,
    style: { getPropertyValue: (key) => properties.get(key) || "", setProperty: (key, value) => { writes++; properties.set(key, value); } },
  };
  const context = vm.createContext({
    window: { innerWidth: 1600, innerHeight: 900, addEventListener: (name, fn) => { events[name] = fn; } },
    document: { querySelectorAll: () => [layer] },
    requestAnimationFrame: (fn) => { frames.set(++frameId, fn); return frameId; },
  });
  const start = code.indexOf("function measureLiveGlassPosition(");
  vm.runInContext(code.slice(start, code.indexOf("function renderLiveSliceToGlass(", start)), context);
  const paint = () => { const jobs = [...frames.values()]; frames.clear(); jobs.forEach((fn) => fn()); };
  for (let n = 0; n < 100; n++) events.pointermove({ buttons: 1 });
  assert.equal(frames.size, 1, "pointer bursts share one display update");
  assert.equal(writes, 0);
  x = 240; y = 120;
  paint();
  assert.equal(properties.get("--dock-x"), "300px");
  assert.equal(properties.get("--dock-y"), "150px");
  assert.equal(canvas.style.width, "2000px");
  assert.equal(canvas.style.height, "1125px");
  events.pointermove({ buttons: 1 }); paint();
  assert.equal(writes, 2, "unchanged geometry does not invalidate styles");
  x = 0; y = 0;
  events.pointerup(); paint();
  assert.equal(properties.get("--dock-x"), "0px");
  assert.equal(properties.get("--dock-y"), "0px");
  events.pointermove({ buttons: 0 });
  assert.equal(frames.size, 0, "hover does not start position work");
});
