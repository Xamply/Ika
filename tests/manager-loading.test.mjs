import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const between = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

test("manager navigation shares pending reads; explicit refresh reads after old work completes", async () => {
  const jobs = [];
  const context = vm.createContext({ readInstance: (i) => {
    const job = deferred(); jobs.push({ ...job, path: i.path }); return job.promise;
  } });
  vm.runInContext(between("  const instanceLoads =", "  function retainInstanceData("), context);
  const i = { path: "instance" };
  const pending = context.loadInstance(i, true, true);
  for (let n = 0; n < 100; n++) assert.equal(context.loadInstance(i, true, true), pending);
  const refresh = context.loadInstance(i, true);
  assert.equal(jobs.length, 1);
  const other = context.loadInstance({ path: "other" }, true, true);
  assert.equal(jobs.length, 2);
  jobs[0].resolve(); jobs[1].resolve();
  await Promise.all([pending, other]);
  assert.equal(jobs.length, 3);
  jobs[2].reject(new Error("disk unavailable"));
  await assert.rejects(refresh, /disk unavailable/);
  const retry = context.loadInstance(i, true, true);
  assert.equal(jobs.length, 4);
  jobs[3].resolve(); await retry;
});

test("unchanged manager scans retain DOM inputs; failures retain last successful data", async () => {
  let failed = false, description = "original";
  const modsCache = new Map(), packsCache = new Map(), configsCache = new Map(), cfgFilesCache = new Map();
  const responses = () => ({
    scan_directory: { mods: [{ filepath: "a.jar", description }] },
    list_instance_packs: { resourcepacks: [], shaders: [] },
    list_instance_configs: ["a.json"], list_instance_config_files: [{ path: "a.json", size: 10 }],
  });
  const context = vm.createContext({
    modsCache, packsCache, configsCache, cfgFilesCache, detailsCache: new Map([["instance", {}]]),
    packsFrom: (value) => value, invoke: async (command) => {
      if (failed) throw new Error("disk unavailable");
      return responses()[command];
    },
    modsSig: () => "mods", packsSig: () => "packs", syncInstanceLiveCounts() {},
    S: { sel: null }, checkUpdates() {}, loadInstalledProjects() {},
  });
  vm.runInContext(between("const instanceScansInFlight =", "// Application State"), context);
  vm.runInContext(between("  const instanceLoads =", "  async function reloadPacks("), context);
  const i = { path: "instance", mods_path: "mods" };
  await context.loadInstance(i, true);
  const mods = modsCache.get(i.path).mods, packs = packsCache.get(i.path), configs = configsCache.get(i.path), files = cfgFilesCache.get(i.path);
  await context.loadInstance(i, true);
  assert.equal(modsCache.get(i.path).mods, mods);
  assert.equal(packsCache.get(i.path), packs);
  assert.equal(configsCache.get(i.path), configs);
  assert.equal(cfgFilesCache.get(i.path), files);
  description = "updated metadata";
  await context.loadInstance(i, true);
  assert.notEqual(modsCache.get(i.path).mods, mods);
  const latest = modsCache.get(i.path);
  failed = true;
  await context.loadInstance(i, true);
  assert.equal(modsCache.get(i.path), latest);
  assert.equal(packsCache.get(i.path), packs);
  assert.equal(configsCache.get(i.path), configs);
  assert.equal(cfgFilesCache.get(i.path), files);
});

test("manager facts and pack lists survive revisits, but changed metadata invalidates them", () => {
  let builds = 0;
  const i = { path: "instance", loader: "Fabric" }, body = {}, S = { tab: "base", mode: "installed" };
  const meta = { notes: "old" }, packs = { shader_active: null }, packInfo = { icons: {} };
  const context = vm.createContext({
    $: () => body, S, modsOf: () => null, updatesCache: new Map(), configsCache: new Map(), issuesCache: new Map(),
    packsCache: new Map([[i.path, packs]]), detailsCache: new Map(), metaCache: new Map([[i.path, meta]]),
    packInfoCache: new Map([[i.path, packInfo]]), busy: new Set(), renderConfig: () => builds++, renderShaders: () => builds++,
  });
  vm.runInContext(between("  let bodyResumeState =", "  function render({ reuseBody"), context);
  context.renderBody(i);
  for (let n = 0; n < 500; n++) context.renderBody(i, true);
  assert.equal(builds, 1);
  meta.notes = "changed";
  context.renderBody(i, true); assert.equal(builds, 2);
  S.tab = "shaders";
  context.renderBody(i, true); assert.equal(builds, 3);
  for (let n = 0; n < 500; n++) context.renderBody(i, true);
  assert.equal(builds, 3);
  packInfo.icons.hash = "new-icon";
  context.renderBody(i, true); assert.equal(builds, 4);
  packs.shader_active = "new.zip";
  context.renderBody(i, true); assert.equal(builds, 5);
});

test("background updates patch indicators without rebuilding mods; focused issues invalidate", () => {
  let builds = 0, patches = 0;
  const i = { path: "instance" }, S = { tab: "mods", mode: "installed" };
  const mods = [], updatesCache = new Map(), issuesCache = new Map(), configsCache = new Map();
  const context = vm.createContext({
    $: () => ({}), S, modsOf: () => mods, updatesCache, issuesCache, configsCache,
    renderMods: () => builds++, syncModUpdateDots: () => patches++,
  });
  vm.runInContext(between("  let bodyResumeState =", "  function render({ reuseBody"), context);
  context.renderBody(i);
  updatesCache.set(i.path, {});
  issuesCache.set(i.path, { missing: [] });
  configsCache.set(i.path, ["new config"]);
  context.renderBody(i, true);
  assert.equal(builds, 1);
  updatesCache.get(i.path).hash = { version: "new" };
  context.renderBody(i, true);
  assert.equal(builds, 1);
  assert.equal(patches, 1);
  updatesCache.clear();
  context.renderBody(i, true);
  assert.equal(builds, 1);
  assert.equal(patches, 2);
  S.focus = "issues";
  context.renderBody(i, true);
  assert.equal(builds, 2);
  issuesCache.get(i.path).missing.push({ name: "required dependency" });
  context.renderBody(i, true);
  assert.equal(builds, 3);
});

test("update indicators add and remove in place without replacing rows or icons", () => {
  let added = 0, removed = 0;
  const rows = ["a.jar", "b.jar"].map((path) => {
    const row = { dataset: { k: path }, dot: null };
    row.querySelector = (selector) => selector === ".mgr-upd-dot" ? row.dot : {
      after(dot) { added++; row.dot = dot; dot.remove = () => { removed++; row.dot = null; }; },
    };
    return row;
  });
  const updatesCache = new Map([["instance", { a: { version: "new" } }]]);
  const context = vm.createContext({ updatesCache,
    modsOf: () => [{ filepath: "a.jar", sha1: "a" }, { filepath: "b.jar", sha1: "b" }],
    document: { createElement: () => ({}) },
  });
  vm.runInContext(between("  function syncModUpdateDots(", "  let bodyResumeState ="), context);
  const body = { querySelectorAll: () => rows }, i = { path: "instance" };
  context.syncModUpdateDots(i, body);
  assert.equal(added, 1); assert.equal(rows[0].dot.title, "Actualización disponible");
  context.syncModUpdateDots(i, body);
  assert.equal(added, 1);
  updatesCache.set(i.path, { b: {} });
  context.syncModUpdateDots(i, body);
  assert.equal(added, 2); assert.equal(removed, 1);
  assert.equal(rows[0].dot, null); assert.ok(rows[1].dot);
});

test("manager discards obsolete show continuations before they start more disk reads", async () => {
  let reads = 0;
  const metas = [], i = { path: "instance" };
  const context = vm.createContext({
    ensurePlates() {}, fitPlates() {}, atmoWatch() {}, render() {}, renderShell() {}, closeMenus() {},
    plates: { start() {}, stop() {} }, all: () => [i], state: { currentView: "manager" },
    clearTimeout() {}, S: {}, instView: {}, homeInstancesList: [i], homeCurrentInstanceIndex: 0,
    syncHomeInstanceByPath() {}, $: () => ({}), requestAnimationFrame() {},
    ensureMeta: () => { const job = deferred(); metas.push(job); return job.promise; },
    sel: () => i, modsCache: new Map(), loadInstance: async () => { reads++; },
  });
  const start = source.indexOf("  let showRetry = 0;", source.indexOf("const mgr ="));
  const end = source.indexOf("    // Opens the section on that instance", start);
  vm.runInContext(source.slice(start, end).replace("  return {", "  globalThis.section = {") + "};", context);
  const old = context.section.onShow();
  context.section.onHide();
  const fresh = context.section.onShow();
  metas[0].resolve(false); await old;
  assert.equal(reads, 0);
  metas[1].resolve(false); await fresh;
  assert.equal(reads, 1);
});
