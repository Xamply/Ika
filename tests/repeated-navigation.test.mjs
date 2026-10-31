import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const code = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const between = (start, end) => code.slice(code.indexOf(start), code.indexOf(end, code.indexOf(start)));
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };

test("manager and comparator share concurrent scans without caching stale results or errors", async () => {
  const jobs = [];
  const context = vm.createContext({ invoke: () => {
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    jobs.push({ resolve, reject });
    return promise;
  } });
  vm.runInContext(between("const instanceScansInFlight =", "// Application State"), context);
  const first = context.scanInstanceDirectory("mods");
  for (let n = 0; n < 100; n++) assert.equal(context.scanInstanceDirectory("mods"), first);
  assert.equal(jobs.length, 1);
  jobs[0].resolve({ mods: [] });
  await first;
  const second = context.scanInstanceDirectory("mods");
  assert.equal(jobs.length, 2);
  jobs[1].reject(new Error("disk unavailable"));
  await assert.rejects(second, /disk unavailable/);
  const retry = context.scanInstanceDirectory("mods");
  assert.equal(jobs.length, 3);
  const afterEdit = context.scanInstanceDirectory("mods", true);
  assert.equal(jobs.length, 3, "refresh after an edit waits for the earlier disk read");
  jobs[2].resolve({ mods: [] });
  await retry;
  assert.equal(jobs.length, 4);
  jobs[3].resolve({ mods: ["updated"] });
  assert.deepEqual(await afterEdit, { mods: ["updated"] });
  await assert.rejects(context.scanInstanceDirectory(""), /Ruta de mods/);
});

test("500 server revisits keep cards; data and search changes invalidate them", () => {
  let builds = 0;
  const servers = [{ id: "one", name: "Server", ip: "localhost", category: "Survival" }];
  const el = { inputSearchServers: { value: "" }, servidoresCardsGrid: {
    set innerHTML(value) { builds++; }, querySelectorAll: () => [],
  } };
  const context = vm.createContext({ el, srvCurrentCategoryFilter: "all", initServidoresViewEvents() {}, getServersList: () => servers });
  vm.runInContext(between("let renderedServersKey =", "// Dock menu (three dots):"), context);
  for (let n = 0; n < 500; n++) context.renderServersView();
  assert.equal(builds, 1);
  servers[0].name = "Changed";
  context.renderServersView();
  assert.equal(builds, 2);
  el.inputSearchServers.value = "no matches";
  context.renderServersView();
  context.renderServersView();
  assert.equal(builds, 3);
});

test("500 comparator revisits preserve right-panel rows and icon indexes", () => {
  let writes = 0, icons = 0, data = { versions: Array.from({ length: 300 }, (_, n) => ({ a: { name: String(n) } })) };
  const list = { els: [], clientHeight: 100,
    set innerHTML(value) { writes++; this.els = Array.from({ length: (value.match(/row/g) || []).length }, () => ({ offsetHeight: 46 })); },
    querySelectorAll() { return this.els; },
  };
  const nodes = new Map([["cmp-vdiff-list", list]]);
  const context = vm.createContext({
    current: () => data, rows: [{}], weighing: false, sides: { a: null, b: null },
    window: { innerWidth: 1200, innerHeight: 800, devicePixelRatio: 1 },
    $: (key) => { if (!nodes.has(key)) nodes.set(key, {}); return nodes.get(key); },
    message: () => "empty", weighingHtml: () => "loading", escapeHtml: (text) => text,
    pairRow: (pair) => { context.rows.push(pair.a); return "row"; }, loadIcons: () => icons++,
  });
  vm.runInContext(between("  const summaryRowBudget =", "  function summaryHtml("), context);
  vm.runInContext(between("  let rightRows = null;", "  // Java the instance uses:"), context);
  for (let n = 0; n < 500; n++) context.renderRight();
  assert.equal(writes, 1);
  assert.equal(icons, 1);
  assert.equal(context.rows.length, 26, "summary must not build all 300 offscreen rows");
  assert.equal(list.els.filter((row) => !row.hidden).length, 2);
  list.clientHeight = 200;
  context.renderRight();
  assert.equal(writes, 1);
  assert.equal(list.els.filter((row) => !row.hidden).length, 4);
  data = { versions: [{ a: { name: "new" } }] };
  context.renderRight();
  assert.equal(writes, 2);
  assert.equal(context.rows.length, 2);
});

test("rapid comparator returns share pending work; explicit rescans remain possible", async () => {
  const jobs = [];
  const context = vm.createContext({ pairKey: () => "a|b", comparePair: () => {
    const job = deferred(); jobs.push(job); return job.promise;
  } });
  vm.runInContext(between("  let pendingRun = null;", "  async function comparePair("), context);
  const first = context.run({ quiet: true });
  for (let n = 0; n < 100; n++) assert.equal(context.run({ quiet: true }), first);
  assert.equal(jobs.length, 1);
  const manual = context.run({ force: true });
  assert.equal(jobs.length, 2);
  jobs[0].resolve(); await first;
  assert.equal(context.run({ quiet: true }), manual, "older completion must not clear newer work");
  jobs[1].resolve(); await manual;
  context.run({ quiet: true });
  assert.equal(jobs.length, 3);
  jobs[2].resolve();
});

test("unchanged disk rechecks reuse comparison; changed metadata and manual requests recompute", async () => {
  let comparisons = 0;
  const a = [{ filepath: "a.jar", sha1: "a", description: "old" }], b = [{ filepath: "b.jar", sha1: "b" }];
  const disk = new Map([["a", a], ["b", b]]);
  const result = { pair: "a|b", mods1: a, mods2: b };
  const context = vm.createContext({
    scans: new Map([["a", { mods: a }], ["b", { mods: b }]]), pendingScan: new Map(),
    sides: { a: { mods_path: "a" }, b: { mods_path: "b" } }, result, runId: 0, weighing: false,
    pairKey: () => "a|b", renderPan() {}, renderAll() {}, derive: () => ({}),
    invoke: async (command, args) => {
      if (command === "scan_directory") return { mods: structuredClone(disk.get(args.path)) };
      comparisons++; return {};
    },
  });
  vm.runInContext(between("const instanceScansInFlight =", "// Application State"), context);
  vm.runInContext(between("  function scan(inst, force)", "  function derive(r)"), context);
  await context.run({ force: true, quiet: true });
  assert.equal(comparisons, 0);
  assert.equal(context.result, result);
  disk.set("a", [{ ...a[0], description: "changed" }]);
  await context.run({ force: true, quiet: true });
  assert.equal(comparisons, 1);
  await context.run({ force: true });
  assert.equal(comparisons, 2);
});

test("100 console returns share one stats query and retain only one poll timer", async () => {
  let writes = 0;
  const jobs = [], timers = new Map(); let timerId = 0;
  const nodes = new Map();
  const context = vm.createContext({
    state: { currentView: "consola" }, instPath: "instance", consoleGeneration: 0, detailsCache: new Map(), refreshFiles() {},
    $: (key) => {
      if (!nodes.has(key)) nodes.set(key, { style: {}, set textContent(value) { writes++; } });
      return nodes.get(key);
    },
    invoke: () => { const job = deferred(); jobs.push(job); return job.promise; },
    setTimeout: (fn) => { timers.set(++timerId, fn); return timerId; }, clearTimeout: (key) => timers.delete(key),
  });
  vm.runInContext(between("  let statsTimer = null;", "  // ---- Counters (per instance"), context);
  const waits = Array.from({ length: 100 }, () => { context.consoleGeneration++; return context.pollStats(); });
  assert.equal(jobs.length, 1);
  jobs[0].resolve({ running: false });
  await Promise.all(waits);
  assert.equal(timers.size, 1);
  assert.equal(writes, 3);
  const pending = context.pollStats();
  context.consoleGeneration++;
  context.state.currentView = "inicio";
  jobs[1].resolve({ running: false });
  await pending;
  assert.equal(timers.size, 0);
  assert.equal(writes, 3, "hidden response must not touch the old section");
});

test("100 early manager opens and closes leave no retry callbacks queued", async () => {
  const timers = new Map(); let id = 0;
  const context = vm.createContext({
    ensurePlates() {}, fitPlates() {}, atmoWatch() {}, render() {}, closeMenus() {},
    plates: { start() {}, stop() {} }, all: () => [], state: { currentView: "manager" },
    setTimeout: (fn) => { timers.set(++id, fn); return id; }, clearTimeout: (key) => timers.delete(key),
  });
  const start = code.indexOf("  let showRetry = 0;", code.indexOf("const mgr ="));
  const end = code.indexOf("    // Opens the section on that instance", start);
  vm.runInContext(code.slice(start, end).replace("  return {", "  globalThis.section = {") + "};", context);
  for (let n = 0; n < 100; n++) {
    await context.section.onShow();
    assert.equal(timers.size, 1);
    context.section.onHide();
    assert.equal(timers.size, 0);
  }
});
