import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const between = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));

function sceneHarness(opens = true) {
  const calls = [];
  const classes = new Set();
  const classList = { add: (name) => classes.add(name), contains: (name) => classes.has(name) };
  const noop = () => {};
  const context = vm.createContext({
    el: {}, document: { body: { classList }, documentElement: { classList: {
      add: () => { throw new Error('native mode must not invalidate the document root'); },
    } } },
    sceneSwitchCounter: 0, weGlassPendingSwitch: 0, sceneSwitchesInFlight: 0,
    weGlassPaintedSwitch: 0, currentSceneBlobUrl: null,
    console: { error: noop }, performance: { now: () => 0 },
    cancelSeamlessLoop: noop, captureCurrentWallpaperSnapshot: async () => { calls.push("snapshot"); },
    getSavedWeProperties: () => ({}),
    invoke: async (name) => {
      calls.push(name);
      if (name === "play_we_wallpaper") return opens;
      if (name === "is_we_scene_rendering") {
        // Capture has produced a frame even though screen sampling still says "not ready".
        assert.ok(calls.includes("start-capture"));
        context.weGlassPaintedSwitch = context.sceneSwitchCounter;
        return false;
      }
      throw new Error(name);
    },
    setVideoGlass: noop, syncConsoleBlurRect: noop,
    startWeCanvasStream: () => { calls.push("start-capture"); context.weGlassPaintedSwitch = context.sceneSwitchCounter; },
    fadeOutSnapshot: (delay) => { calls.push(`fade:${delay}`); },
    setTimeout: () => { throw new Error("ready frames must not wait for fallback timers"); },
    saveActiveWallpaperState: () => { calls.push("save"); },
    updateWePillsBlurBackdrop: () => { calls.push("preview"); },
    loadActiveWallpaperTransform: noop,
    updateActiveCatalogCards: noop, updateActiveSteamCards: noop, updateActiveDownloadedCards: noop,
    isBackgroundLive: () => true, showToast: () => { calls.push("error"); },
  });
  // The 600 ms outgoing-video cleanup is scheduled independently of readiness.
  context.setTimeout = (_fn, ms) => {
    assert.equal(ms, 600, "ready captures must not wait for polling/fallback timers");
  };
  vm.runInContext(between("async function playSceneWallpaper(", "// Dual Video Smooth Transition Engine"), context);
  return { context, calls, classes };
}

test("first captured scene frame reveals wallpaper without screen-readiness polling", async () => {
  const { context, calls } = sceneHarness();
  await context.playSceneWallpaper("scene.pkg", "Scene");
  for (let i = 0; i < 6; i++) await Promise.resolve();
  assert.ok(calls.includes("start-capture"));
  assert.ok(!calls.includes("is_we_scene_rendering"));
  assert.ok(calls.includes("fade:0"), JSON.stringify(calls));
  assert.equal(calls.filter((name) => name === "save").length, 1);
  assert.equal(calls.filter((name) => name === "preview").length, 1);
  assert.ok(!calls.includes("error"));
  assert.equal(context.sceneSwitchesInFlight, 0);
});

test("failed native opening does not start capture or persist the failed wallpaper", async () => {
  const { context, calls, classes } = sceneHarness(false);
  await context.playSceneWallpaper("missing.pkg", "Missing");
  assert.ok(calls.includes("error"));
  assert.ok(!calls.includes("start-capture"));
  assert.ok(!calls.includes("save"));
  assert.ok(!classes.has("has-we-active"));
  assert.equal(context.sceneSwitchesInFlight, 0);
});

test("PKG replacement keeps the old scene until the snapshot completes four browser presentation frames", async () => {
  const { context, calls, classes } = sceneHarness(false);
  classes.add('has-we-active');
  const frames = [];
  const bytes = new Uint8Array(12);
  const header = new DataView(bytes.buffer);
  header.setUint32(0, 1, true);
  header.setUint32(4, 1, true);
  const previousInvoke = context.invoke;
  Object.assign(context, {
    window: { innerWidth: 100, innerHeight: 100 },
    requestAnimationFrame: callback => { frames.push(callback); },
    prepareSnapshotCanvas: () => ({ drawImage: () => calls.push('draw-snapshot') }),
    showSnapshotCanvas: () => calls.push('show-snapshot'),
    decodeSnapshotBitmap: async () => ({ close() {} }),
    invoke: async name => name === 'capture_we_snapshot' ? bytes : previousInvoke(name),
  });
  context.el.homeBgSnapshot = {};
  vm.runInContext(between('function waitForPaint(', 'function showSnapshotCanvas('), context);
  vm.runInContext(between('async function captureCurrentWallpaperSnapshot(', 'function fadeOutSnapshot('), context);
  const pending = context.playSceneWallpaper('next.pkg', 'Next');
  const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
  await settle();
  assert.ok(calls.includes('draw-snapshot'));
  assert.ok(calls.includes('show-snapshot'));
  for (let frame = 0; frame < 4; frame++) {
    assert.ok(!calls.includes('play_we_wallpaper'), `old scene was replaced before presentation frame ${frame + 1}`);
    assert.equal(frames.length, 1);
    frames.shift()();
    await settle();
  }
  await pending;
  assert.ok(calls.includes('play_we_wallpaper'));
});

test("switching active wallpaper or playlist updates downloaded cards without rebuilding catalog", async () => {
  const item = { id: "1", video_path: "video.mp4", title: "Video" };
  const signature = `${item.id}|${item.video_path}|${item.project_path}|${item.title}|${item.preview_path}|${item.source}|${item.is_scene}|${item.resolution}|${item.file_size_mb}|0`;
  let updates = 0;
  const context = vm.createContext({
    el: { downloadedCatalogGrid: { childElementCount: 1 } },
    downloadedCardsSig: signature, hasWeCustomProperties: () => false,
    updateActiveDownloadedCards: () => { updates++; },
  });
  vm.runInContext(between("async function renderDownloadedCards(", "function updateActiveDownloadedCards("), context);
  await context.renderDownloadedCards([item]);
  await context.renderDownloadedCards([item]);
  assert.equal(updates, 2);
});
