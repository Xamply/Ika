import test from "node:test";
import assert from "node:assert/strict";
import { fitHomePosition } from "../src/home-layout.js";

test("legacy maximized layout fits the smaller workspace without forgetting its original position", () => {
  const home = Object.freeze({ left: 1768, top: 304, width: 650, height: 405 });
  const small = { width: 1536, height: 865 };
  const large = { width: 2560, height: 1382 };
  const smallSize = { width: 460, height: 292 };
  const expected = fitHomePosition(home, small, smallSize, 150);
  assert.ok(expected.left > 990 && expected.left < 1000);
  assert.ok(expected.top + smallSize.height < 500);
  for (let i = 0; i < 1000; i++) {
    assert.deepEqual(fitHomePosition(home, large, home), { left: 1768, top: 304 });
    assert.deepEqual(fitHomePosition(home, small, smallSize, 150), expected);
  }
});

test("a position chosen in windowed mode is exact on restore and remains inside the maximized workspace", () => {
  const home = { left: 980, top: 150, width: 460, height: 292, viewportWidth: 1536, viewportHeight: 865 };
  assert.deepEqual(fitHomePosition(home, { width: 1536, height: 865 }, home, 150), { left: 980, top: 150 });
  const fitted = fitHomePosition(home, { width: 2560, height: 1382 }, { width: 650, height: 405 });
  assert.ok(fitted.left + 650 < 2560);
  assert.ok(fitted.top + 405 < 1382 - 168);
});

test("offscreen and oversized saved windows stay reachable without modifying the saved coordinates", () => {
  const home = Object.freeze({ left: 9000, top: 9000 });
  assert.deepEqual(fitHomePosition(home, { width: 1536, height: 865 }, { width: 2000, height: 1000 }), { left: 10, top: 60 });
});
