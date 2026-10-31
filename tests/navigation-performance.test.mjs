import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const code = readFileSync(new URL("../src/main.js", import.meta.url), "utf8").replace(/\r\n/g, "\n");
const between = (start, end) => code.slice(code.indexOf(start), code.indexOf(end, code.indexOf(start)));

test("comparator empty and loading states retain DOM across repeated returns", () => {
  let builds = 0, result = null;
  const center = { dataset: {}, querySelector: () => null, set innerHTML(value) { builds++; } };
  const context = vm.createContext({
    $: (key) => key === "cmp-center" ? center : null,
    current: () => result, filter: "summary", query: "", weighing: false,
    sides: { a: null, b: null }, downloading: "", rows: [],
    document: { activeElement: null }, summaryHtml: () => "summary", fullHtml: () => "full",
    fitPanels() {}, loadIcons() {},
  });
  vm.runInContext(between("  let renderedCenterState =", "  let searchDebounce = null;"), context);
  for (let n = 0; n < 500; n++) context.renderCenter();
  assert.equal(builds, 1);
  context.weighing = true;
  context.renderCenter();
  assert.equal(builds, 2);
  result = { only1: [] };
  context.renderCenter();
  assert.equal(builds, 3);
  context.query = "sodium";
  context.renderCenter();
  assert.equal(builds, 4, "new results and searches still invalidate the content");
});

test("graphics warm-up prepares comparator before manager without waiting for instance discovery", async () => {
  const warmed = [];
  const section = (name) => ({ prewarm: async () => warmed.push(name), onHide() {} });
  const view = () => ({ classList: { add() {}, remove() {} } });
  const context = vm.createContext({
    windowShown: Promise.resolve(), PREWARM_START_DELAY_MS: 0, PREWARM_PAINT_MS: 0,
    homeInstancesList: [], state: { currentView: "inicio" }, console,
    el: { viewComparador: view(), viewManager: view() },
    window: { ikaComparer: section("comparador"), ikaManager: section("manager") },
    setTimeout: (fn) => fn(), requestAnimationFrame: (fn) => fn(),
  });
  vm.runInContext(between("async function prewarmSections(", "function updateViewSwitcherGlider("), context);
  await context.prewarmSections();
  assert.deepEqual(warmed, ["comparador", "manager"]);
});

test("section switch renders servers once and measures navigation after mutations", () => {
  const writes = [], frames = new Map(), timers = new Map();
  let callbackId = 0;
  let rendered = 0, measured = 0;
  const element = (name) => ({ classList: {
    toggle: () => writes.push(name), add: () => writes.push(name), contains: () => false,
  } });
  const el = Object.fromEntries([
    "viewInicio", "viewConsola", "viewComparador", "viewManager", "viewServidores",
    "rightInspectorPanel", "panelResizer", "subHeaderBar", "btnCopyMissing", "btnCopyAll",
    "btnReset", "headerRight", "bottomInspector",
  ].map((name) => [name, element(name)]));
  el.viewSwitcher = { querySelectorAll: () => [] };
  const noop = () => {};
  const context = vm.createContext({
    el, state: { currentView: "inicio" }, homeInstancesList: [], homeCurrentInstanceIndex: 0,
    document: { getElementById: () => ({ dataset: {}, classList: { toggle() {} } }), body: element("body") },
    window: { ikaManager: { onShow() { assert.ok(writes.includes("bottomInspector")); }, onHide: noop } },
    syncBgVideoPlayback: noop, syncConsoleBlurRect: noop, viewShowsBackground: () => true,
    updateInstanceSpecsBar: noop, updateInstPillGlider: noop, updateWePillsBlurBackdrop: noop,
    stopHcpDinoGame: noop, updateHomeConsolePreview: noop, updateConsoleBadge: noop,
    renderCurrentView: () => rendered++, renderServersView: () => rendered++,
    requestAnimationFrame: (fn) => { frames.set(++callbackId, fn); return callbackId; },
    cancelAnimationFrame: (id) => frames.delete(id),
    setTimeout: (fn) => { timers.set(++callbackId, fn); return callbackId; },
    clearTimeout: (id) => timers.delete(id),
    updateViewSwitcherGlider: () => measured++,
  });
  vm.runInContext(between("let sectionShowFrame =", "function getFormattedTime("), context);
  vm.runInContext('switchView("servidores")', context);
  assert.equal(rendered, 0, "section population waits until navigation can paint");
  assert.equal(measured, 0);
  const paint = () => { const jobs = [...frames.values()]; frames.clear(); jobs.forEach((fn) => fn()); };
  const populate = () => { const jobs = [...timers.values()]; timers.clear(); jobs.forEach((fn) => fn()); };
  paint();
  assert.equal(measured, 1);
  assert.equal(rendered, 0);
  populate();
  assert.equal(rendered, 1);
  vm.runInContext('switchView("manager")', context);
  const nativeCalls = [];
  context.document.body.classList.contains = () => true;
  context.invoke = (command, args) => { nativeCalls.push({ command, ...args }); return Promise.resolve(); };
  context.viewShowsBackground = () => ["inicio", "manager", "comparador", "consola", "servidores"].includes(context.state.currentView);
  for (let n = 0; n < 100; n++) context.switchView(n % 2 ? "inicio" : "manager");
  assert.equal(nativeCalls.length, 0, "same wallpaper visibility must not reposition native windows");
  context.switchView("missing");
  context.switchView("manager");
  assert.deepEqual(nativeCalls, [
    { command: "sync_we_window_rect", visible: false },
    { command: "sync_we_window_rect", visible: true },
  ]);
  paint(); populate();
  assert.equal(rendered, 2, "rapid navigation must populate only the last selected section");
});

test("native console synchronization skips unchanged visibility and keeps explicit force", () => {
  let dockVisible = false;
  const calls = [];
  const context = vm.createContext({
    localStorage: { getItem: (key) => key === "ika_extras_win32_window" ? "true" : null },
    document: { body: { classList: { contains: () => false } } },
    isConsoleDockVisible: () => dockVisible,
    invoke: (command, args) => { calls.push({ command, ...args }); return Promise.resolve(); },
  });
  vm.runInContext(between("let isBlurSyncInFlight =", 'window.addEventListener("resize", () => {\n  syncConsoleBlurRect();'), context);
  for (let n = 0; n < 100; n++) context.syncConsoleBlurRect(true);
  assert.equal(calls.length, 1); assert.equal(calls[0].visible, false);
  dockVisible = true; context.syncConsoleBlurRect(true);
  assert.equal(calls.length, 2); assert.equal(calls[1].visible, true);
  context.syncConsoleBlurRect(true, true); assert.equal(calls.length, 3);
  dockVisible = false; context.syncConsoleBlurRect(true);
  assert.equal(calls.length, 4); assert.equal(calls[3].visible, false);
});

test("repeated tab hover leaves parked sections untouched; click still navigates", () => {
  let sectionWrites = 0;
  const switched = [];
  const buttons = ["manager", "comparador"].map((view) => {
    const button = new EventTarget();
    button.dataset = { view };
    return button;
  });
  const switcher = new EventTarget();
  switcher.querySelectorAll = () => buttons;
  const view = { classList: {
    add() { sectionWrites++; }, remove() { sectionWrites++; },
  } };
  const context = vm.createContext({
    el: { viewSwitcher: switcher, viewManager: view, viewComparador: view },
    switchView: (name) => switched.push(name), initHexIconBounce() {},
    setTimeout() { throw new Error("Tab hover must not schedule section work"); },
  });
  vm.runInContext(between("  // Tab switcher", "  // Every window opens"), context);
  for (let n = 0; n < 1000; n++) {
    const event = new Event("pointerover");
    Object.defineProperty(event, "target", { value: buttons[n % 2] });
    switcher.dispatchEvent(event);
    switcher.dispatchEvent(new Event("pointerleave"));
  }
  assert.equal(sectionWrites, 0);
  assert.equal(switched.length, 0);
  buttons.forEach((button) => button.dispatchEvent(new Event("click")));
  assert.deepEqual(switched, ["manager", "comparador"]);
});

test("comparator fits all panels with one layout flush", () => {
  let dirty = false, layouts = 0;
  const measure = (value) => { if (dirty) { layouts++; dirty = false; } return value; };
  const item = () => ({
    set hidden(v) { this.value = v; dirty = true; }, get hidden() { return this.value; },
    get offsetHeight() { return measure(20); },
  });
  const panels = [5, 0, 8].map((count) => {
    const rows = Array.from({ length: count }, item), more = item();
    return { rows, more, list: { dataset: { total: count === 5 ? 5000 : count },
      querySelectorAll: () => rows, parentElement: { querySelector: () => more },
      get clientHeight() { return measure(60); },
    } };
  });
  const window = { innerWidth: 1200, innerHeight: 800, devicePixelRatio: 1 };
  const context = vm.createContext({ window, stage: { querySelectorAll: () => panels.map((p) => p.list) }, CHEVRON_DOWN: ">" });
  vm.runInContext(between("  function fitPanels(", "  function loadIcons("), context);
  vm.runInContext("fitPanels()", context);
  assert.equal(layouts, 1);
  assert.equal(panels[0].rows.filter((r) => !r.hidden).length, 3);
  assert.equal(panels[0].more.innerHTML, "Ver 4997 más >", "count includes rows never built in the summary");
  assert.equal(panels[1].more.hidden, true);
  assert.equal(panels[2].rows.filter((r) => !r.hidden).length, 3);
  for (let n = 0; n < 1000; n++) context.fitPanels(true);
  assert.equal(layouts, 1, "unchanged returns must not reveal or measure hidden rows");
  window.innerHeight = 600;
  context.fitPanels(true);
  assert.equal(layouts, 2);
});

test("manager reuses installed mods but invalidates changed data and handles no instances", () => {
  let builds = 0;
  const instance = { path: "instance" }, body = {};
  const mods = new Map([[instance.path, []]]);
  const S = { tab: "mods", mode: "installed" };
  const context = vm.createContext({
    $: () => body, S, modsOf: (i) => mods.get(i.path),
    updatesCache: new Map(), configsCache: new Map(), issuesCache: new Map(),
    renderMods: () => builds++, emptyHtml: () => "empty",
  });
  vm.runInContext(between("  let bodyResumeState =", "  function render({ reuseBody"), context);
  context.instance = instance;
  vm.runInContext("renderBody(instance); renderBody(instance, true)", context);
  assert.equal(builds, 1);
  mods.set(instance.path, [{ filepath: "new.jar" }]);
  vm.runInContext("renderBody(instance, true)", context);
  assert.equal(builds, 2);
  S.mq = "new search";
  vm.runInContext("renderBody(instance, true); renderBody(instance)", context);
  assert.equal(builds, 4);
  vm.runInContext("renderBody(null, true)", context);
  assert.equal(body.innerHTML, "empty");
});

test("reopening cached comparison keeps result DOM and stale data still rescans", () => {
  let rebuilt = 0, refreshed = 0, rescans = 0;
  const scans = new Map([["mods-a", { at: Date.now() }], ["mods-b", { at: Date.now() }]]);
  const context = vm.createContext({
    scans, sides: { a: { mods_path: "mods-a" }, b: { mods_path: "mods-b" } },
    result: { pair: "a|b" }, pairKey: () => "a|b", filter: "summary",
    fitStage() {}, keo: { setRunning() {} }, restorePair() {}, allInstances: () => [{}], clearTimeout() {},
    renderAll: () => rebuilt++, renderPan: () => refreshed++, renderRight() {}, fitPanels() {},
    run: ({ force }) => { assert.equal(force, true); rescans++; },
  });
  const start = code.indexOf("  let showRetries = 0;");
  const end = code.indexOf("    onHide() {", start);
  vm.runInContext(code.slice(start, end).replace("  return {", "  globalThis.section = {") + "};", context);
  vm.runInContext("section.onShow(); section.onShow()", context);
  assert.equal(rebuilt, 0);
  assert.equal(refreshed, 4);
  assert.equal(rescans, 0);
  scans.set("mods-a", { at: 0 });
  vm.runInContext("section.onShow()", context);
  assert.equal(rescans, 1);
});
