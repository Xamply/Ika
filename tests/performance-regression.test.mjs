import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");

test("animation clock cancels hidden work and resumes subscriptions once", () => {
  const frames = new Map(), timers = new Map(), events = {};
  let id = 0, now = 100, ticks = 0;
  const document = { hidden: false, addEventListener: (name, fn) => { events[name] = fn; } };
  const context = vm.createContext({
    document, performance: { now: () => now }, console,
    requestAnimationFrame: (fn) => { frames.set(++id, fn); return id; },
    cancelAnimationFrame: (key) => frames.delete(key),
    setTimeout: (fn) => { timers.set(++id, fn); return id; },
    clearTimeout: (key) => timers.delete(key),
  });
  vm.runInContext(source("frame-clock.js").replaceAll("export ", ""), context);
  context.countTick = () => ticks++;
  vm.runInContext("globalThis.stop = onTick(countTick)", context);
  const fire = () => {
    const [key, fn] = frames.entries().next().value;
    frames.delete(key);
    fn(now);
  };
  fire();
  assert.equal(ticks, 1);
  assert.equal(timers.size, 1);
  document.hidden = true;
  events.visibilitychange();
  assert.equal(timers.size + frames.size, 0);
  vm.runInContext("globalThis.stop2 = onTick(() => {})", context);
  assert.equal(timers.size + frames.size, 0);
  now += 10000;
  document.hidden = false;
  events.visibilitychange();
  events.visibilitychange();
  assert.equal(frames.size, 1);
  fire();
  assert.equal(ticks, 2);
  vm.runInContext("stop(); stop2()", context);
  assert.equal(timers.size + frames.size, 0);
});

test("console batch bounds DOM work and preserves scroll and literal log text", () => {
  let reads = 0, writes = 0;
  class Element {
    children = [];
    appendChild(child) {
      if (child.fragment) {
        for (const item of child.children) this.appendChild(item);
      } else {
        this.children.push(child);
        child.parent = this;
      }
    }
    remove() { this.parent.children.shift(); }
    querySelector() { return null; }
    get childElementCount() { return this.children.length; }
    get firstElementChild() { return this.children[0]; }
    get scrollHeight() { reads++; return this.children.length * 20; }
    get clientHeight() { reads++; return 100; }
    get scrollTop() { reads++; return this.top || 0; }
    set scrollTop(value) { writes++; this.top = value; }
  }
  const container = new Element();
  const document = {
    createElement: () => new Element(),
    createDocumentFragment: () => Object.assign(new Element(), { fragment: true }),
  };
  const context = vm.createContext({ document, logContainer: container, MAX_LINES: 300, stopDino() {} });
  const code = source("console_window.js");
  vm.runInContext(code.slice(code.indexOf("function appendLogLines("), code.indexOf("function startDino(")), context);
  context.lines = Array.from({ length: 1000 }, (_, i) => ({ level: "WARN", text: `<tag>${i}`, ts: 1000 }));
  vm.runInContext("appendLogLines(lines)", context);
  assert.equal(container.children.length, 300);
  assert.equal(container.children[0].children[1].textContent, "<tag>700");
  assert.equal(container.children[299].children[1].textContent, "<tag>999");
  assert.equal(reads, 4);
  assert.equal(writes, 1);
  container.top = 0;
  reads = writes = 0;
  vm.runInContext("appendLogLines(lines.slice(0, 10))", context);
  assert.equal(container.children.length, 300);
  assert.equal(reads, 3);
  assert.equal(writes, 0);
});

test("3D scene keeps one draw in flight and then uses current animation time", () => {
  const messages = [];
  const context = vm.createContext({ worker: { postMessage: (m) => messages.push(m) } });
  const code = source("manager-plates.js");
  const start = code.indexOf("      draw(t) {", code.indexOf("    world = {"));
  const end = code.indexOf("\n    };", start);
  vm.runInContext(`let ready = false, drawing = false; globalThis.world = {${code.slice(start, end)}}`, context);
  vm.runInContext("world.draw(1); ready = true; world.draw(2); world.draw(3); world.draw(4)", context);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].t, 2);
  vm.runInContext("drawing = false; world.draw(5)", context);
  assert.equal(messages.length, 2);
  assert.equal(messages[1].t, 5);
});

test("3D worker acknowledges both bitmap and direct-canvas draws", () => {
  const messages = [];
  const context = vm.createContext({ self: { postMessage: (m) => messages.push(m) } });
  const code = source("manager-world.worker.js");
  vm.runInContext(code.slice(code.indexOf("let world = null"), code.indexOf("self.onmessage =")), context);
  vm.runInContext("world = { draw() {} }; canvas = { width: 10, transferToImageBitmap: () => ({}) }; frame()", context);
  assert.equal(messages[0].type, "drawn");
  vm.runInContext("bitmaps = true; frame()", context);
  assert.equal(messages[1].type, "frame");
  vm.runInContext("canvas.width = 2; frame()", context);
  assert.equal(messages[2].type, "drawn");
});
