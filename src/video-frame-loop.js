// Two players can overlap during a crossfade. Merge their frame notifications into
// one paint and never resample a stationary video on the decorative animation clock.
export function startVideoFrameLoop({ videos, paint, isActive }) {
  let running = true, raf = 0, lastPresentedAt = -Infinity;
  const callbacks = new Map();
  const schedule = () => {
    if (!running || raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      if (running && isActive()) paint();
    });
  };
  for (const video of videos) {
    if (typeof video.requestVideoFrameCallback !== "function") continue;
    const next = (now, metadata) => {
      if (!running) return;
      const presentedAt = metadata?.expectedDisplayTime ?? now;
      // rVFC already runs in the video's presentation phase. Waiting for another
      // RAF added a display frame of lag and let the two clocks drift apart.
      // Both players may notify the same presentation during a crossfade.
      if (isActive() && Math.abs(presentedAt - lastPresentedAt) > 0.5) {
        lastPresentedAt = presentedAt;
        paint();
      }
      callbacks.set(video, video.requestVideoFrameCallback(next));
    };
    callbacks.set(video, video.requestVideoFrameCallback(next));
  }
  // Compatibility for older WebViews; modern WebView2 uses decoded-frame callbacks.
  let fallback = 0;
  const tick = () => {
    if (!running) return;
    schedule();
    fallback = setTimeout(tick, 1000 / 60);
  };
  if (!callbacks.size) tick();
  schedule();
  return () => {
    running = false;
    cancelAnimationFrame(raf);
    clearTimeout(fallback);
    for (const [video, id] of callbacks) video.cancelVideoFrameCallback(id);
    callbacks.clear();
  };
}
