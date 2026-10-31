// Validate stored values without restricting movement to the viewport edges.
export function normalizeWallpaperTransform(transform) {
  const finite = (value, fallback) => Number.isFinite(value) ? value : fallback;
  const scale = Math.max(0.4, Math.min(4, finite(transform.scale, 1)));
  return {
    scale,
    panX: Math.max(-1_000_000, Math.min(1_000_000, finite(transform.panX, 0))),
    panY: Math.max(-1_000_000, Math.min(1_000_000, finite(transform.panY, 0))),
  };
}
