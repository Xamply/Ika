import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const start = source.indexOf("  const catalogPages =");
const end = source.indexOf("  async function latestVersion(", start);

test("catalog revisits reuse pages and obsolete searches cannot overwrite newer results", async () => {
  const requests = [];
  const S = { src: "modrinth", dq: "first", mode: "download", sel: "instance" };
  const context = vm.createContext({
    dl: { hits: [], loading: false }, S, state: { currentView: "manager" },
    dlKey: () => S.dq, renderBody() {},
    searchModrinth: () => {
      let resolve, reject;
      const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
      requests.push({ resolve, reject });
      return promise;
    },
  });
  vm.runInContext(source.slice(start, end), context);
  const instance = { path: "instance", loader: "Fabric", mc_version: "1.21" };
  const old = context.search(instance);
  S.dq = "second";
  const latest = context.search(instance);
  requests[1].resolve({ hits: ["new"], total: 1 });
  await latest;
  requests[0].resolve({ hits: ["old"], total: 1 });
  await old;
  assert.deepEqual(context.dl.hits, ["new"]);
  for (let n = 0; n < 100; n++) await context.search(instance);
  assert.equal(requests.length, 2, "returning to cached catalog does not start more network requests");
  S.dq = "failure";
  const failed = context.search(instance);
  requests[2].reject(new Error("offline"));
  await failed;
  assert.match(context.dl.error, /offline/);
  const retry = context.search(instance);
  assert.equal(requests.length, 4, "failed requests are evicted so retry can recover");
  requests[3].resolve({ hits: ["recovered"], total: 1 });
  await retry;
  assert.deepEqual(context.dl.hits, ["recovered"]);
});
