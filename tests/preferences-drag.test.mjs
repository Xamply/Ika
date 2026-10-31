import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

test("preferences drag batches pointer moves without layout reads and commits latest position", () => {
  const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const start = source.indexOf("  // Draggable settings modal window");
  const code = source.slice(start, source.indexOf("  let currentAppearanceSec", start));
  const events = {}, boxEvents = {}, frames = new Map(), stored = new Map(), classes = new Set();
  let reads = 0, sequence = 0;
  const container = {
    getBoundingClientRect: () => { reads++; return { left: 0, top: 0 }; },
    get clientWidth() { reads++; return 1200; },
    get clientHeight() { reads++; return 900; },
  };
  const box = {
    style: {}, offsetParent: container,
    get offsetWidth() { reads++; return 600; },
    get offsetHeight() { reads++; return 500; },
    getBoundingClientRect: () => { reads++; return { left: 100, top: 100 }; },
    classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name) },
    addEventListener: (name, callback) => { boxEvents[name] = callback; },
    setPointerCapture() {}, releasePointerCapture() {},
  };
  const context = vm.createContext({
    modalBoxEl: box,
    window: { addEventListener: (name, callback) => { events[name] = callback; }, getSelection: () => null },
    document: { body: { style: { userSelect: "text" } } },
    requestAnimationFrame: (callback) => { frames.set(++sequence, callback); return sequence; },
    cancelAnimationFrame: (id) => frames.delete(id),
    localStorage: { setItem: (key, value) => stored.set(key, value) },
  });
  vm.runInContext(code, context);
  boxEvents.pointerdown({ button: 0, pointerId: 7, clientX: 120, clientY: 120, preventDefault() {},
    target: { closest: (selector) => selector.startsWith("button") ? null : {} } });
  const initialReads = reads;
  for (let i = 0; i < 100; i++) events.pointermove({ pointerId: 7, clientX: 220 + i, clientY: 220 });
  assert.equal(reads, initialReads, "no geometry reads during movement");
  assert.equal(frames.size, 1, "one paint for a burst of pointer events");
  assert.equal(box.style.left, "100px", "movement does not write layout position");
  const paint = [...frames.values()][0]; frames.clear(); paint();
  assert.equal(box.style.translate, "199px 100px");
  events.pointermove({ pointerId: 7, clientX: 5000, clientY: 5000 });
  events.pointerup({ pointerId: 8 });
  assert.ok(classes.has("is-dragging"), "another pointer cannot end the drag");
  events.pointerup({ pointerId: 7 });
  assert.equal(box.style.left, "594px");
  assert.equal(box.style.top, "394px");
  assert.equal(box.style.translate, "");
  assert.equal(frames.size, 0, "release commits latest event even before the queued frame");
  assert.equal(stored.get("ika_pref_pos_left"), "594");
  assert.equal(context.document.body.style.userSelect, "text");
  boxEvents.pointerdown({ button: 0, pointerId: 9, clientX: 120, clientY: 120, preventDefault() {},
    target: { closest: (selector) => selector.startsWith("button") ? null : {} } });
  events.pointermove({ pointerId: 9, clientX: 150, clientY: 150 });
  events.blur({});
  assert.equal(frames.size, 0);
  assert.equal(classes.size, 0, "losing window focus ends dragging");
});
