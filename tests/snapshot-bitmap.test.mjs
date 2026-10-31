import assert from 'node:assert/strict';
import test from 'node:test';
import { decodeSnapshotBitmap } from '../src/snapshot-bitmap.js';

test('snapshot uses top-down BGRX bitmap with exact pixels, including offset buffers', async () => {
  const storage = new Uint8Array(32);
  const bytes = storage.subarray(4, 20);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  view.setUint32(0, 1, true);
  view.setUint32(4, 2, true);
  bytes.set([3, 2, 1, 0, 6, 5, 4, 0], 8);
  const previous = globalThis.createImageBitmap;
  globalThis.createImageBitmap = async blob => {
    assert.equal(blob.type, 'image/bmp');
    const buffer = await blob.arrayBuffer();
    const bmp = new DataView(buffer);
    assert.equal(buffer.byteLength, 62);
    assert.equal(bmp.getUint16(0, true), 0x4d42);
    assert.equal(bmp.getUint32(10, true), 54);
    assert.equal(bmp.getInt32(18, true), 1);
    assert.equal(bmp.getInt32(22, true), -2);
    assert.equal(bmp.getUint16(28, true), 32);
    assert.deepEqual([...new Uint8Array(buffer, 54)], [3, 2, 1, 0, 6, 5, 4, 0]);
    return 'decoded';
  };
  try { assert.equal(await decodeSnapshotBitmap(bytes), 'decoded'); }
  finally { globalThis.createImageBitmap = previous; }
});

test('snapshot rejects truncated headers, oversized dimensions and incomplete pixels before decoding', async () => {
  await assert.rejects(decodeSnapshotBitmap(new Uint8Array(7)), /header/);
  for (const [width, height, length] of [[0, 2, 16], [20000, 1, 16], [2, 2, 16], [16384, 16384, 16]]) {
    const raw = new ArrayBuffer(length);
    const view = new DataView(raw);
    view.setUint32(0, width, true); view.setUint32(4, height, true);
    await assert.rejects(decodeSnapshotBitmap(raw), /dimensions or pixels/);
  }
});
