import assert from "node:assert/strict";
import test from "node:test";
import { createVisibleTaskQueue } from "../src/visible-tasks.js";

test("offscreen previews wait; only two decoders run and removed cards are skipped", async () => {
  let observer;
  const previous = globalThis.IntersectionObserver;
  globalThis.IntersectionObserver = class {
    constructor(callback) { this.callback = callback; this.targets = new Set(); observer = this; }
    observe(target) { this.targets.add(target); }
    unobserve(target) { this.targets.delete(target); }
  };
  try {
    const enqueue = createVisibleTaskQueue(2);
    const cards = Array.from({ length: 6 }, () => ({ isConnected: true }));
    const started = [], releases = [];
    for (const [index, card] of cards.entries()) enqueue(card, () => {
      started.push(index);
      return new Promise((resolve) => releases.push(resolve));
    });
    await Promise.resolve();
    assert.equal(started.length, 0);
    cards[2].isConnected = false;
    observer.callback(cards.slice(0, 5).map((target) => ({ target, isIntersecting: true })));
    await Promise.resolve();
    assert.deepEqual(started, [0, 1]);
    releases.shift()();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(started, [0, 1, 3]);
    releases.splice(0).forEach((resolve) => resolve());
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(started, [0, 1, 3, 4]);
    releases.splice(0).forEach((resolve) => resolve());
    assert.equal(observer.targets.size, 1, "remaining offscreen card stays deferred");
  } finally {
    globalThis.IntersectionObserver = previous;
  }
});
