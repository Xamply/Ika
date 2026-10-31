import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import { createButtonPreviewCache } from '../src/button-video-preview.js';

test('mini preview shares pending generation and never returns original source on unavailable encoder', async () => {
  let calls = 0, finish;
  const cache = createButtonPreviewCache(() => {
    calls++;
    return new Promise(resolve => { finish = resolve; });
  }, path => `asset:${path}`);
  const first = cache('4k.mp4');
  assert.equal(cache('4k.mp4'), first);
  finish('tiny.mp4');
  assert.equal(await first, 'asset:tiny.mp4');
  assert.equal(await cache('4k.mp4'), 'asset:tiny.mp4');
  assert.equal(calls, 1);
  const unavailable = createButtonPreviewCache(async () => null, path => path);
  assert.equal(await unavailable('4k.mp4'), null);
  assert.equal(await unavailable(null), null);
});

const source = readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
const playSource = source.slice(source.indexOf('async function playMiniPreview('), source.indexOf('\nfunction isMp4Url('));

test('leaving while clip is generated cannot open decoder later', async () => {
  let finish;
  const video = { dataset: { currentSrc: 'original.mp4' }, play() { throw new Error('late playback'); } };
  const context = vm.createContext({
    resolveMiniPreview: () => new Promise(resolve => { finish = resolve; }),
    miniHovered: true, miniHoverGeneration: 1, currentMiniSlot: 'primary',
    el: { btnWallpaperMiniVideo: video },
  });
  vm.runInContext(playSource, context);
  const playback = context.playMiniPreview(video, 'original.mp4', 1);
  context.miniHovered = false;
  context.miniHoverGeneration++;
  finish('tiny.mp4');
  await playback;
  assert.equal(video.src, undefined);
});

test('late initial clip cannot replace newer carousel selection', async () => {
  let finish;
  const video = { dataset: { currentSrc: 'original.mp4' }, play() { throw new Error('stale playback'); } };
  const context = vm.createContext({
    resolveMiniPreview: () => new Promise(resolve => { finish = resolve; }),
    miniHovered: true, miniHoverGeneration: 1, currentMiniSlot: 'primary',
    el: { btnWallpaperMiniVideo: video, btnWallpaperMiniVideoNext: {} },
  });
  vm.runInContext(playSource, context);
  const playback = context.playMiniPreview(video, 'original.mp4', 1);
  context.currentMiniSlot = 'secondary';
  finish('tiny.mp4');
  await playback;
  assert.equal(video.src, undefined);
});
