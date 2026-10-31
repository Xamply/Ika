import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

test('video glass follows decoded frames, merges overlapping players and cancels late frames', () => {
  const frames = new Map(); let id = 0, paints = 0, active = true;
  const videos = Array.from({ length: 2 }, () => {
    const callbacks = new Map();
    return { callbacks,
      requestVideoFrameCallback(fn) { callbacks.set(++id, fn); return id; },
      cancelVideoFrameCallback(key) { callbacks.delete(key); },
      decode(at) { const jobs = [...callbacks.values()]; callbacks.clear(); jobs.forEach(fn => fn(at, { expectedDisplayTime: at })); },
    };
  });
  const context = vm.createContext({
    videos, paint: () => paints++, isActive: () => active,
    requestAnimationFrame(fn) { frames.set(++id, fn); return id; },
    cancelAnimationFrame(key) { frames.delete(key); },
    setTimeout() { throw new Error('Decoded-frame path must not poll on a timer'); }, clearTimeout() {},
  });
  vm.runInContext(readFileSync(new URL('../src/video-frame-loop.js', import.meta.url), 'utf8').replace('export ', '') +
    '\nglobalThis.stop = startVideoFrameLoop({ videos, paint, isActive });', context);
  const display = () => { const jobs = [...frames.values()]; frames.clear(); jobs.forEach(fn => fn()); };
  display(); assert.equal(paints, 1);
  for (let n = 0; n < 240; n++) display();
  assert.equal(paints, 1, 'no redundant paints without a decoded frame');
  for (let n = 0; n < 60; n++) { videos.forEach(v => v.decode(n * 1000 / 60)); assert.equal(frames.size, 0); }
  assert.equal(paints, 61, '60 fps video is not capped to decorative 30 fps clock');
  active = false; videos[0].decode(1100); display(); assert.equal(paints, 61);
  active = true; videos[1].decode(1200); display(); assert.equal(paints, 62);
  const late = [...videos[0].callbacks.values()][0];
  context.stop(); late(1300, {expectedDisplayTime:1300}); display();
  assert.equal(paints, 62);
  assert.equal(frames.size, 0);
  assert.ok(videos.every(v => v.callbacks.size === 0));
});
