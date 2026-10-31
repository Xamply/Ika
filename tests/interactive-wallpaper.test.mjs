import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const between = (start, end) => main.slice(main.indexOf(start), main.indexOf(end, main.indexOf(start)));
const flush = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

function frameHarness(capture, paint, isActive = () => true) {
  const frames = new Map(), timers = new Map(); let id = 0;
  const context = vm.createContext({
    requestAnimationFrame: (fn) => { frames.set(++id, fn); return id; },
    cancelAnimationFrame: (key) => frames.delete(key),
    setTimeout: (fn) => { timers.set(++id, fn); return id; }, clearTimeout: (key) => timers.delete(key),
    capture, paint, isActive,
  });
  const source = readFileSync(new URL("../src/latest-frame-loop.js", import.meta.url), "utf8");
  vm.runInContext(source.replace("export ", "") + "\nglobalThis.stop = startLatestFrameLoop({ capture, paint, isActive });", context);
  return { context, frames, timers, fire: (now) => {
    const jobs = [...frames.values()]; frames.clear(); jobs.forEach((fn) => fn(now));
  } };
}

test("interactive capture runs near 60 fps on 60/144 Hz displays and paints on next display frame", async () => {
  for (const refreshRate of [60, 144]) {
    let captures = 0, paints = 0;
    const h = frameHarness(async () => ++captures, () => paints++);
    for (let frame = 0; frame <= refreshRate; frame++) {
      h.fire(frame * 1000 / refreshRate);
      await flush();
    }
    assert.ok(captures >= 59 && captures <= 61, `${refreshRate} Hz: ${captures} capture requests`);
    assert.ok(paints >= 58 && paints <= 61);
    assert.equal(h.frames.size, 1);
    h.context.stop();
    assert.equal(h.frames.size, 0);
  }
});

test("slow capture never queues requests; stopping discards late responses", async () => {
  let complete, captures = 0, paints = 0;
  const h = frameHarness(() => { captures++; return new Promise((resolve) => { complete = resolve; }); }, () => paints++);
  for (let frame = 0; frame < 100; frame++) { h.fire(frame * 16.67); await flush(); }
  assert.equal(captures, 1);
  h.context.stop();
  complete("stale");
  await flush();
  assert.equal(paints, 0);
  assert.equal(h.frames.size, 0);
  assert.equal(h.timers.size, 0);
});

test("invisible glass idles instead of capturing or spinning on every refresh", async () => {
  let active = false, captures = 0;
  const h = frameHarness(async () => ++captures, () => {}, () => active);
  h.fire(0); await flush();
  assert.equal(captures, 0);
  assert.equal(h.frames.size, 0);
  assert.equal(h.timers.size, 1);
  active = true;
  const wake = [...h.timers.values()][0]; h.timers.clear(); wake();
  h.fire(100); await flush();
  assert.equal(captures, 1);
  h.context.stop();
});

test("identical glass looks filter a source frame once; new frames and dimensions invalidate it", () => {
  let draws = 0, allocations = 0;
  const context = vm.createContext({ document: { createElement: () => {
    allocations++;
    return { width: 0, height: 0, getContext: () => ({ clearRect() {}, drawImage() { draws++; } }) };
  } } });
  vm.runInContext(between("function getBlurredGlassFrame(", "function renderLiveSliceToGlass("), context);
  const source = { width: 256, height: 144, _glassFrameVersion: 1 };
  const first = context.getBlurredGlassFrame(source, "blur(2px) saturate(1)");
  for (let i = 0; i < 20; i++) assert.equal(context.getBlurredGlassFrame(source, "blur(2px) saturate(1)"), first);
  assert.equal(draws, 1);
  assert.equal(allocations, 1);
  source._glassFrameVersion++;
  context.getBlurredGlassFrame(source, "blur(2px) saturate(1)");
  assert.equal(draws, 2);
  source.width = 320;
  context.getBlurredGlassFrame(source, "blur(2px) saturate(1)");
  assert.equal(draws, 3);
  for (let i = 0; i < 40; i++) context.getBlurredGlassFrame(source, `blur(${i}px)`);
  assert.equal(source._glassBlurCache.size, 8, "resizing panels cannot grow blur cache without bound");
});

test("options expansion applies layout once and animates transform; reversal cancels prior animation", async () => {
  const classes = new Set(); const animations = []; let reads = 0;
  const box = {
    classList: { toggle: (key, on) => on ? classes.add(key) : classes.delete(key) },
    checkVisibility: () => true, getAnimations: () => [],
    getBoundingClientRect: () => { reads++; return { left: 0, top: 0, width: classes.has("has-we-props-open") ? 1500 : 1200, height: 800 }; },
    animate: (keyframes, options) => {
      let resolve;
      const animation = { keyframes, options, finished: new Promise((done) => { resolve = done; }), cancelled: false,
        effect: { getComputedTiming: () => ({ progress: 0.5 }) },
        cancel() { this.cancelled = true; resolve(); }, resolve: () => resolve() };
      animations.push(animation); return animation;
    },
  };
  const panelClasses = new Set(["hidden"]);
  const context = vm.createContext({
    el: { modalWallpapersBox: box, wePropertiesPanel: { classList: { contains: key => panelClasses.has(key), toggle: (key, on) => on ? panelClasses.add(key) : panelClasses.delete(key) } },
      modalWallpapers: { classList: { contains: () => false } } },
    window: { innerWidth: 2560, matchMedia: () => ({ matches: false }) },
  });
  vm.runInContext(between("let wePropertiesRequest =", "function closeWePropertiesPanel("), context);
  const opening = context.setWePropertiesPanelOpen(true);
  assert.equal(reads, 0, "expansion must not force layout before or after changing width");
  assert.equal(panelClasses.has("hidden"), false);
  assert.equal(animations[0].keyframes[0].transform, `scaleX(${1260/1620})`);
  assert.ok(animations[0].keyframes.every((frame) => !("width" in frame) && !("maxWidth" in frame)));
  const closing = context.setWePropertiesPanelOpen(false);
  assert.equal(animations[0].cancelled, true);
  assert.equal(animations[1].keyframes[0].transform, `scaleX(${1440/1260})`, "rapid reversal starts at the current visible width");
  animations[1].resolve();
  await Promise.all([opening, closing]);
  assert.equal(animations[1].cancelled, true);
  assert.equal(panelClasses.has("hidden"), true);
});

test("detached options preparation shares work, invalidates saved edits and bounds retained panels", async () => {
  let builds = 0;
  const saved = new Map();
  const context = vm.createContext({
    getWallpaperStorageKey: item => item.id,
    getSavedWeProperties: item => saved.get(item.id) || {},
    buildWePropertiesPanel: async item => { builds++; return [{ item }]; },
  });
  vm.runInContext(between("const wePropertiesPanels =", "function prepareVisibleWallpaperOptions("), context);
  const item = { id: "first", properties: {} };
  const first = context.prepareWePropertiesPanel(item);
  assert.equal(context.prepareWePropertiesPanel(item), first);
  const nodes = await first;
  assert.equal(await context.prepareWePropertiesPanel(item), nodes);
  assert.equal(builds, 1);
  saved.set(item.id, { speed: 2 });
  assert.notEqual(await context.prepareWePropertiesPanel(item), nodes);
  item.properties = { changed: true };
  await context.prepareWePropertiesPanel(item);
  assert.equal(builds, 3);
  for (let n=0;n<30;n++) await context.prepareWePropertiesPanel({id:String(n),properties:{}});
  assert.equal(vm.runInContext("wePropertiesPanels.size",context),8);
});
