import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const between = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));

test("rapid wallpaper switches retain incoming video; loop preloading cannot replace outgoing video during fade", async () => {
  const noop = () => {};
  const video = (src, active) => {
    const classes = new Set(active ? ["is-active"] : []);
    return {
      src, paused: false, readyState: 2, videoWidth: 1920, style: {},
      getAttribute: () => src,
      classList: { add: (key) => classes.add(key), remove: (key) => classes.delete(key), contains: (key) => classes.has(key) },
      addEventListener: noop, removeEventListener: noop,
      play() { this.paused = false; return Promise.resolve(); },
      pause() { this.paused = true; },
    };
  };
  const primary = video("old.mp4", true), secondary = video("old.mp4", false);
  const timers = new Map(); let timerId = 0;
  const context = vm.createContext({
    el: { homeBgVideo: primary, homeBgVideoNext: secondary },
    document: { body: { classList: { contains: () => false } } },
    sceneSwitchCounter: 0, wallpaperVideoTransition: 0, weGlassPendingSwitch: 0,
    currentActiveVideoSlot: "primary", currentSceneBlobUrl: null,
    activeWallpaperUrl: "old.mp4", activeWallpaperTitle: "Old", bgPlaylistTransition: "crossfade",
    seamlessLoopInProgress: false,
    localStorage: { setItem: noop },
    cancelSeamlessLoop: noop, captureCurrentWallpaperSnapshot: async () => true,
    loadActiveWallpaperTransform: noop, isMp4Url: () => true, updateButtonWallpaperPreview: noop,
    fadeOutSnapshot: noop, updateActiveCatalogCards: noop,
    setTimeout: (fn) => { timers.set(++timerId, fn); return timerId; },
    clearTimeout: (id) => timers.delete(id),
  });
  vm.runInContext(between("function preloadInactivePlayer(", "function triggerSeamlessVideoLoop("), context);
  vm.runInContext(between("async function transitionBackground(", "function setMbgActiveTag("), context);
  await context.transitionBackground("first.mp4", "First");
  assert.equal(context.currentActiveVideoSlot, "secondary", "active slot changes before fade cleanup");
  context.preloadInactivePlayer(secondary);
  assert.equal(primary.src, "old.mp4", "outgoing video stays intact for the fade");
  const oldCleanup = [...timers.values()][0];
  await context.transitionBackground("second.mp4", "Second");
  assert.equal(primary.src, "second.mp4");
  assert.equal(context.currentActiveVideoSlot, "primary");
  oldCleanup();
  assert.equal(primary.paused, false, "stale cleanup cannot pause latest wallpaper");
  assert.equal(primary.src, "second.mp4");
  const newCleanup = [...timers.values()].at(-1);
  newCleanup();
  assert.equal(secondary.paused, true);
  assert.equal(context.wallpaperVideoTransition, 0);
  assert.equal(secondary.src, "second.mp4", "loop preload resumes only after handoff");
});
