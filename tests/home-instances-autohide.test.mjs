import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

test("idle viewer hides after two seconds, stays visible on hover/focus, and saves its setting", () => {
  const classes = new Set();
  const events = {}, changes = {};
  let pending, delay, saved, focus = null;
  const zone = {
    matches: () => false,
    contains: (element) => element === zone,
    classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name) },
    addEventListener: (name, callback) => { events[name] = callback; },
  };
  const toggle = { checked: false, addEventListener: (name, callback) => { changes[name] = callback; } };
  const context = vm.createContext({
    document: {
      getElementById: (id) => id === "home-instance-switcher-zone" ? zone : id === "setting-ui-autohide-home-instances" ? toggle : null,
      get activeElement() { return focus; },
    },
    localStorage: { getItem: () => "true", setItem: (_key, value) => { saved = value; } },
    setTimeout: (callback, ms) => { pending = callback; delay = ms; },
    clearTimeout: () => { pending = null; },
    queueMicrotask: (callback) => callback(),
  });
  const source = readFileSync(new URL("../src/home-instances-autohide.js", import.meta.url), "utf8");
  vm.runInContext(source.replace("export function", "function") + "\ninstallHomeInstancesAutohide();", context);
  assert.equal(delay, 2000);
  pending();
  assert.ok(classes.has("instances-idle-hidden"));
  events.pointerenter();
  assert.equal(classes.size, 0);
  assert.equal(pending, null);
  events.pointerleave();
  assert.equal(typeof pending, "function");
  focus = zone;
  events.focusin();
  assert.equal(pending, null);
  focus = null;
  events.focusout();
  pending();
  assert.ok(classes.has("instances-idle-hidden"));
  toggle.checked = false;
  changes.change();
  assert.equal(saved, "false");
  assert.equal(classes.size, 0);
  assert.equal(pending, null);
});
