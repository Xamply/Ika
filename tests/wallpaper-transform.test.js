import test from "node:test";
import assert from "node:assert/strict";
import { normalizeWallpaperTransform } from "../src/wallpaper-transform.js";

test("free panning survives at every zoom level, beyond either viewport edge", () => {
  for (const scale of [0.4, 1, 1.05, 1.5, 4]) {
    for (const panX of [-10000, -1600, 0, 1600, 10000]) {
      for (const panY of [-10000, 0, 10000]) {
        assert.deepEqual(normalizeWallpaperTransform({ scale, panX, panY }), { scale, panX, panY });
      }
    }
  }
});

test("invalid stored numbers cannot reach CSS or native geometry", () => {
  assert.deepEqual(normalizeWallpaperTransform({ scale: NaN, panX: Infinity, panY: -Infinity }),
    { scale: 1, panX: 0, panY: 0 });
  assert.deepEqual(normalizeWallpaperTransform({ scale: 50, panX: 1e100, panY: -1e100 }),
    { scale: 4, panX: 1000000, panY: -1000000 });
});
