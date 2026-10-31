import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import { normalizeWallpaperTransform } from '../src/wallpaper-transform.js';

const source = readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
const between = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));

for (const catalog of ['MotionBGs', 'Steam']) {
  test(`${catalog} selection keeps playlist labels intact until membership actually changes`, () => {
    let writes = 0, text = '+ Playlist';
    const label = { get textContent() { return text; }, set textContent(value) { writes++; text = value; } };
    const button = {
      classList: { toggle() {} }, querySelector: () => label,
      set innerHTML(_) { throw new Error('selection replaced a playlist label'); },
    };
    const card = {
      dataset: { fileSlug: 'scene', videoPath: 'scene.mp4' },
      classList: { toggle() {} },
      querySelector: selector => selector.includes('playlist') ? button : null,
    };
    const grid = { querySelectorAll: () => [card] };
    const context = vm.createContext({
      el: { mbgCatalogGrid: grid, steamCatalogGrid: grid },
      activeWallpaperUrl: 'scene.mp4', bgPlaylist: [], updateActiveDownloadedCards() {},
    });
    const steam = catalog === 'Steam';
    vm.runInContext(steam
      ? between('function updateActiveSteamCards(', 'let downloadedWallpapers =')
      : between('function updateActiveCatalogCards(', 'function playBackgroundVideoSafely('), context);
    const update = steam ? context.updateActiveSteamCards : context.updateActiveCatalogCards;
    for (let i = 0; i < 1000; i++) update();
    assert.equal(writes, 0);
    context.bgPlaylist.push({ url: 'scene.mp4' });
    update();
    assert.equal(text, '✓ Playlist');
    assert.equal(writes, 1);
    update();
    assert.equal(writes, 1);
  });
}

test('video glass handoff changes tint without synchronously resolving intermediate native styles', () => {
  const style = () => {
    const values = new Map();
    return { setProperty: (k, v) => values.set(k, v), removeProperty: k => values.delete(k), values };
  };
  const ids = ['inst-pill-we-blur-bg', 'inst-search-we-blur-bg'];
  const layers = new Map(ids.map(id => [id, { style: style(), parentElement: { style: style() } }]));
  const context = vm.createContext({
    VIDEO_GLASS_TARGETS: ids,
    document: { getElementById: id => layers.get(id) },
    getComputedStyle: () => { throw new Error('forced style resolution during handoff'); },
  });
  vm.runInContext(between('function applyVideoGlassTints(', 'function setVideoGlass('), context);
  context.applyVideoGlassTints(true);
  for (const layer of layers.values()) assert.equal(layer.parentElement.style.values.get('background'), 'transparent');
  assert.equal(layers.get(ids[0]).style.values.get('--video-glass-tint'), 'none');
  assert.equal(layers.get(ids[1]).style.values.get('--video-glass-tint'), 'rgba(8, 8, 14, 0.84)');
  context.applyVideoGlassTints(false);
  for (const layer of layers.values()) {
    assert.ok(!layer.style.values.has('--video-glass-tint'));
    assert.ok(!layer.parentElement.style.values.has('background'));
  }
});

test('wallpaper pan/zoom stays local to the canvas and native transforms still use their own geometry', () => {
  let native = false;
  const calls = [];
  const video = () => ({ classList: { toggle: (...args) => calls.push(args) } });
  const canvas = { style: {} };
  const context = vm.createContext({
    el: { homeBgVideo: video(), homeBgVideoNext: video(), homeBgScene: video() },
    document: {
      body: { classList: { contains: () => native } },
      documentElement: { style: { setProperty: () => { throw new Error('global inherited transform invalidation'); } } },
      getElementById: id => id === 'home-bg-canvas' ? canvas : null,
    },
    wallpaperTransform: { scale: 1.75, panX: -91, panY: 44, fit: 'contain' },
    normalizeWallpaperTransform,
    sendWeTransformThrottled: (...args) => calls.push(args),
  });
  vm.runInContext(between('function applyWallpaperTransform(', 'function updateWallpaperHud('), context);
  context.applyWallpaperTransform(false, false, true);
  assert.equal(canvas.style.transform, 'translate(-91px, 44px) scale(1.75)');
  assert.equal(calls.filter(args => args[0] === 'is-fit-contain' && args[1] === true).length, 3);
  native = true;
  context.applyWallpaperTransform(false, false, true);
  assert.deepEqual(calls.at(-1), [1.75, -91, 44, true]);
});
