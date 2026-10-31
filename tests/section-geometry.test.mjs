import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

test("comparator preserves canvas backing stores on returns and refits after width or DPR changes", () => {
  const source = readFileSync(new URL("../src/comparador-keo.js", import.meta.url), "utf8");
  let resizes = 0;
  const renderer = { resize() { resizes++; } };
  const stage = { style: {} };
  const window = { devicePixelRatio: 1, addEventListener() {} };
  const context = vm.createContext({
    root: { querySelector: (selector) => selector === "#kx-stage" ? stage : null },
    W: 1633, H: 261, window, Borders: renderer, OrbLight: renderer, Energy: renderer,
  });
  const tail = source.slice(source.indexOf("  const stageEl ="));
  vm.runInContext(`globalThis.scene = (function () {${tail})();`, context);
  const firstScale = context.scene.fit(1000);
  for (let n = 0; n < 1000; n++) assert.equal(context.scene.fit(1000), firstScale);
  assert.equal(resizes, 3);
  for (const invalid of [0, -1, NaN, Infinity, undefined]) context.scene.fit(invalid);
  assert.equal(resizes, 3);
  context.scene.fit(1100);
  assert.equal(resizes, 6);
  window.devicePixelRatio = 2;
  context.scene.fit(1100);
  assert.equal(resizes, 9);
});

test("manager reuses carousel geometry but remeasures when viewport or panel size changes", () => {
  const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  let reads = 0, height = 600, width = 420;
  const zone = { get clientHeight() { reads++; return height; }, get clientWidth() { reads++; return width; } };
  const window = { innerWidth: 1240, innerHeight: 780 };
  const context = vm.createContext({ $: () => zone, window });
  const start = source.indexOf("  let instanceGeometry = null;");
  vm.runInContext(source.slice(start, source.indexOf("  function placeInstCard(", start)), context);
  const initial = context.instGeometry();
  for (let n = 0; n < 1000; n++) assert.equal(context.instGeometry(), initial);
  assert.equal(reads, 2);
  height = 400;
  window.innerHeight = 580;
  const resized = context.instGeometry();
  assert.notEqual(resized, initial);
  assert.ok(resized.w0 < initial.w0);
  width = 300;
  // The zone's ResizeObserver invalidates this after dragging a panel edge.
  vm.runInContext("instanceGeometry = null", context);
  assert.ok(context.instGeometry().w0 < resized.w0);
  height = 0;
  assert.equal(context.instGeometry(false), null);
});
