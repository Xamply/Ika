// GDI snapshots contain [width u32 LE, height u32 LE, top-down BGRX pixels].
// BMP lets the browser decode those bytes without a full-screen JS channel-swapping loop.
export async function decodeSnapshotBitmap(raw) {
  const bytes = raw instanceof ArrayBuffer ? new Uint8Array(raw) : raw;
  if (!(bytes instanceof Uint8Array) || bytes.byteLength < 8) throw new Error("Invalid snapshot header");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const width = view.getUint32(0, true), height = view.getUint32(4, true);
  const size = width * height * 4;
  if (!width || !height || width > 16384 || height > 16384 || size > 256 * 1024 * 1024 || bytes.byteLength < size + 8) {
    throw new Error("Invalid snapshot dimensions or pixels");
  }
  const header = new ArrayBuffer(54);
  const bmp = new DataView(header);
  bmp.setUint16(0, 0x4d42, true);
  bmp.setUint32(2, 54 + size, true);
  bmp.setUint32(10, 54, true);
  bmp.setUint32(14, 40, true);
  bmp.setInt32(18, width, true);
  bmp.setInt32(22, -height, true);
  bmp.setUint16(26, 1, true);
  bmp.setUint16(28, 32, true);
  bmp.setUint32(34, size, true);
  return createImageBitmap(new Blob([header, bytes.subarray(8, 8 + size)], { type: "image/bmp" }));
}
