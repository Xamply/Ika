import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../src/manager-plates.js", import.meta.url), "utf8");

test("plate masks cover their blur output without full-stage allocations", () => {
  const nodes = [];
  const node = (tag) => {
    const result = { tag, attrs: {}, style: {}, classList: { toggle() {} },
      setAttribute(key, value) { this.attrs[key] = value; }, appendChild() {},
    };
    nodes.push(result); return result;
  };
  const context = vm.createContext({ document: { createElementNS: (_, tag) => node(tag) } });
  const code = source.slice(source.indexOf('const NS ='), source.indexOf('// Mounts the scene.')).replaceAll('export ', '');
  vm.runInContext(code, context);
  context.buildPlates(node("stage"));
  for (let i = 0; i < 4; i++) {
    const filter = (n) => nodes.find((n2) => n2.attrs.id === `mgrp${i}b${n}`).attrs;
    const mask = (suffix) => nodes.find((n2) => n2.attrs.id === `mgrp${i}${suffix}`).attrs;
    for (const [n, suffix] of [[2, "edgeMask"], [3, "mask"], [4, "mask"], [5, "mask"]]) {
      const f = filter(n), m = mask(suffix);
      assert.equal(f.filterUnits, "userSpaceOnUse");
      assert.ok(m.y <= Math.max(0, f.y));
      assert.ok(m.y + m.height >= Math.min(1000, f.y + f.height));
      assert.ok(m.height < 1000);
    }
    assert.equal(mask("edgeMask").height, 256);
    assert.equal(filter(5).height, 592);
  }
});

test("cropped wave surface includes animation, lift, antialiasing and blur", () => {
  const context = vm.createContext({ HALF_H: 96, vn: (x) => .5 + .5 * Math.sin(x) });
  const start = source.indexOf("  const wavePts ="), end = source.indexOf('  // spreadMethod=', start);
  const bounds = source.match(/const WAVES_TOP = (\d+), WAVES_HEIGHT = (\d+);/);
  const top = Number(bounds[1]), bottom = top + Number(bounds[2]);
  vm.runInContext(source.slice(start, end).replace("const wavePts =", "globalThis.wavePts ="), context);
  for (const y of [225, 350, 475, 600]) for (const t of [0, .01, 10, 100000]) {
    const w = { L: { x: 108, y }, F: { x: 500, y: y + 96 }, R: { x: 892, y },
      base: 10, amp: 9, fine: .6, jag: 1.2, spike: 2.2, speed: .3, ph: 100 };
    const pts = context.wavePts(w, t);
    for (let n = 1; n < pts.length; n += 2) {
      // Minimum stage width 260px: half-res antialiasing <4 world units;
      // widest line radius 2.5, four sigma 16, maximum lift + float 18.
      assert.ok(pts[n] - 18 - 4 - 2.5 - 16 >= top);
      assert.ok(pts[n] + 4 + 2.5 + 16 <= bottom);
    }
  }
  assert.ok((bottom - top) / 1000 <= .56);
});
