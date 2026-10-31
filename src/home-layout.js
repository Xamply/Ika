// Legacy home layouts were authored on the maximized 1440p workspace.
export const HOME_REFERENCE_VIEWPORT = { viewportWidth: 2560, viewportHeight: 1382 };

// Keep the saved position immutable: resizing back restores it without rounding drift.
export function fitHomePosition(home, viewport, size, bottomClearance = 168) {
  const referenceWidth = home.viewportWidth || HOME_REFERENCE_VIEWPORT.viewportWidth;
  const referenceHeight = home.viewportHeight || HOME_REFERENCE_VIEWPORT.viewportHeight;
  const left = home.left * Math.max(1, viewport.width - size.width) / Math.max(1, referenceWidth - (home.width || size.width));
  const top = home.top * Math.max(1, viewport.height - 52 - size.height) / Math.max(1, referenceHeight - 52 - (home.height || size.height));
  return {
    left: Math.max(10, Math.min(left, viewport.width - size.width - 10)),
    top: Math.max(60, Math.min(top, viewport.height - 52 - size.height - bottomClearance)),
  };
}
