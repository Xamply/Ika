import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const gameCode = main.slice(main.indexOf("let hcpDinoAnimId ="), main.indexOf("let hcpAutoScroll ="));

function game() {
  const frames = new Map(), idle = new Set(), resizeCallbacks = [];
  let id = 0, time = 1000, draws = [], size = { width: 480, height: 180 }, canvasResets = 0, gameOverY = null;
  const events = () => ({
    handlers: new Map(),
    addEventListener(name, fn) { if (!this.handlers.has(name)) this.handlers.set(name, new Set()); this.handlers.get(name).add(fn); },
    removeEventListener(name, fn) { this.handlers.get(name)?.delete(fn); },
    emit(name, props = {}) {
      for (const fn of this.handlers.get(name) || []) fn({ button: 0, pointerId: 1, preventDefault() {}, ...props });
    },
  });
  const ctx = new Proxy({}, { get(target, key) {
    if (key in target) return target[key];
    if (key === "drawImage") return (...args) => draws.push(args);
    if (key === "fillText") return (text, x, y) => { if (text === "G A M E   O V E R") gameOverY = y; };
    if (key.startsWith("create")) return () => ({ addColorStop() {} });
    return () => {};
  } });
  const elements = [];
  const element = () => Object.assign(events(), {
    style: {}, children: [], classList: { contains: () => false },
    appendChild(child) { this.children.push(child); child.parentElement = this; },
    contains(child) { return this.children.includes(child); },
    remove() {}, setPointerCapture() {}, checkVisibility: () => true,
    getBoundingClientRect: () => ({ ...size, top: 0 }), getContext: () => ctx,
  });
  const document = Object.assign(events(), { hidden: false,
    createElement(tag) {
      const el = element();
      if (tag === "canvas") {
        for (const dimension of ["width", "height"]) {
          let value = 0;
          Object.defineProperty(el, dimension, {
            get: () => value,
            set: (next) => { value = next; canvasResets++; draws = []; },
          });
        }
      }
      elements.push(el);
      return el;
    },
  });
  const window = Object.assign(events(), { devicePixelRatio: 1 });
  const context = vm.createContext({
    document, window, console, DINO_SPRITE_B64: "sprite", Image: class {},
    localStorage: { getItem: () => null, setItem() {} },
    el: { homeConsolePreviewDock: element() },
    requestAnimationFrame: (fn) => { frames.set(++id, fn); return id; },
    cancelAnimationFrame: (key) => frames.delete(key), queueMicrotask: (fn) => fn(),
    onTick: (fn) => { idle.add(fn); return () => idle.delete(fn); },
    ResizeObserver: class {
      constructor(fn) { this.fn = fn; }
      observe() { resizeCallbacks.push(this.fn); }
      disconnect() { resizeCallbacks.splice(resizeCallbacks.indexOf(this.fn), 1); }
    },
    container: element(),
  });
  vm.runInContext(gameCode + "\nstartHcpDinoGame(container)", context);
  const wrap = elements[0];
  const step = () => {
    draws = [];
    time += 16.6667;
    if (frames.size) {
      const [key, fn] = frames.entries().next().value;
      frames.delete(key);
      fn(time);
    } else for (const fn of [...idle]) fn(time);
    const dino = draws.findLast((args) => args[1] >= 848 && args[5] === 28);
    return dino ? { y: dino[6], width: dino[7] } : null;
  };
  return {
    wrap, window, document, step,
    run: (count) => Array.from({ length: count }, step).filter(Boolean),
    resize(height) { size = { ...size, height }; resizeCallbacks.forEach((fn) => fn()); },
    obstacles: () => draws.filter((args) => [228, 245, 332, 357, 134, 180].includes(args[1])),
    stop: () => vm.runInContext("stopHcpDinoGame()", context),
    pending: () => frames.size + idle.size,
    clocks: () => ({ frames: frames.size, idle: idle.size }),
    canvasResets: () => canvasResets,
    hasDrawing: () => draws.length > 0,
    gameOverY: () => gameOverY,
  };
}

function countJumps(points) {
  let jumps = 0, rising = false;
  for (let n = 1; n < points.length; n++) {
    if (points[n].y === points[n - 1].y) continue;
    const nextRising = points[n].y < points[n - 1].y;
    if (nextRising && !rising) jumps++;
    rising = nextRising;
  }
  return jumps;
}

test("primary press jumps at top, center and bottom, including first start", () => {
  for (const clientY of [1, 90, 179]) {
    const g = game();
    g.wrap.emit("pointerdown", { clientY });
    const positions = g.run(5);
    assert.ok(positions.at(-1).y < 113);
    assert.equal(positions.at(-1).width, 44);
    g.stop();
  }
});

test("mid-air click survives release and triggers one jump on landing", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  const positions = g.run(8);
  g.window.emit("pointerup");
  g.wrap.emit("pointerdown");
  g.window.emit("pointerup");
  positions.push(...g.run(100));
  assert.equal(countJumps(positions), 2);
  assert.equal(positions.at(-1).y, 113);
  g.stop();
});

test("holding mouse keeps jumping, releasing outside stops repetition", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  assert.ok(countJumps(g.run(180)) >= 2);
  g.window.emit("pointerup");
  assert.equal(g.run(100).at(-1).y, 113);
  g.stop();
});

test("keyboard release does not cancel a held mouse jump", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  g.window.emit("keydown", { code: "Space" });
  g.window.emit("keyup", { code: "Space" });
  assert.ok(countJumps(g.run(180)) >= 2);
  g.stop();
});

test("focus loss clears held input and cleanup removes listeners and clocks", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  g.run(5);
  g.window.emit("blur");
  assert.equal(g.run(110).at(-1).y, 113);
  g.stop();
  assert.equal(g.pending(), 0);
  for (const target of [g.wrap, g.window, g.document]) {
    for (const listeners of target.handlers.values()) assert.equal(listeners.size, 0);
  }
});

test("existing cactus stays on the ground when panel grows and shrinks", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  g.window.emit("pointerup");
  g.run(200);
  const before = g.obstacles()[0];
  assert.ok(before, "cactus should already have spawned");
  assert.equal(before[6] + before[8], 160);
  g.resize(320);
  g.step();
  const grown = g.obstacles()[0];
  assert.equal(grown[6] + grown[8], 300);
  g.resize(140);
  g.step();
  const shrunk = g.obstacles()[0];
  assert.equal(shrunk[6] + shrunk[8], 120);
  g.stop();
});

test("resizing clears backing canvas only in the frame that repaints it", () => {
  const g = game();
  g.step();
  for (const height of [200, 220, 240, 260]) {
    const resets = g.canvasResets();
    g.resize(height);
    assert.equal(g.canvasResets(), resets, "observer must not clear the displayed frame");
    g.step();
    assert.equal(g.canvasResets(), resets + 1, "only changed height needs a reset");
    assert.ok(g.hasDrawing(), "resizing and drawing must share a frame");
  }
  const resets = g.canvasResets();
  g.resize(260);
  g.step();
  assert.equal(g.canvasResets(), resets, "same-size notifications must not reset canvas");
  g.stop();
});

test("wallpaper shortcuts and unrelated keys leave idle dino asleep", () => {
  const g = game();
  g.step();
  const clocks = g.clocks();
  assert.equal(clocks.frames, 0);
  for (const code of ["ControlLeft", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space", "KeyW"]) {
    g.window.emit("keydown", { code, ctrlKey: true });
    g.window.emit("keyup", { code, ctrlKey: false });
  }
  g.window.emit("wheel", { ctrlKey: true, deltaY: 120 });
  g.wrap.emit("pointerdown", { ctrlKey: true });
  g.window.emit("pointerup");
  g.window.emit("keydown", { code: "KeyA" });
  g.window.emit("keyup", { code: "KeyA" });
  assert.deepEqual(g.clocks(), clocks);
  assert.ok(g.run(40).every((position) => position.y === 113));
  g.stop();
});

test("accepted game key releases normally even when Ctrl is pressed later", () => {
  const g = game();
  g.window.emit("keydown", { code: "Space" });
  g.run(5);
  g.window.emit("keyup", { code: "Space", ctrlKey: true });
  assert.equal(g.run(100).at(-1).y, 113);
  g.stop();
});

test("game over stays in the sky below clouds at every console height", () => {
  const g = game();
  g.wrap.emit("pointerdown");
  g.window.emit("pointerup");
  g.run(400);
  assert.notEqual(g.gameOverY(), null);
  for (const height of [140, 180, 400, 800]) {
    g.resize(height);
    g.step();
    const y = g.gameOverY();
    assert.ok(y - 15 > 43, "text must sit below the cloud band");
    assert.ok(y <= 96, "tall panels must not drag text towards the ground");
  }
  g.stop();
});
