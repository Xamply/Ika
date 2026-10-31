// Wallpaper capture owns one request and one pending frame, never a backlog of old mouse reactions.
export function startLatestFrameLoop({ capture, paint, isActive, interval = 1000 / 60, onError = () => {} }) {
  let running = true, inFlight = false, pending = null, raf = 0, timer = 0, due = 0;
  const schedule = () => { if (running && !raf) raf = requestAnimationFrame(tick); };
  function tick(now) {
    raf = 0;
    if (!running) return;
    const active = isActive();
    if (!running) return;
    if (!active) {
      pending = null;
      timer = setTimeout(() => { timer = 0; due = 0; schedule(); }, 100);
      return;
    }
    if (pending !== null) {
      const frame = pending;
      pending = null;
      try { paint(frame); } catch (error) { onError(error); }
    }
    if (!inFlight && now + 0.5 >= due) {
      inFlight = true;
      due += interval;
      if (due <= now) due = now + interval;
      Promise.resolve().then(capture).then((frame) => {
        if (running && frame != null) pending = frame;
      }).catch(onError).finally(() => { inFlight = false; });
    }
    schedule();
  }
  schedule();
  return () => {
    running = false;
    pending = null;
    cancelAnimationFrame(raf);
    clearTimeout(timer);
    raf = timer = 0;
  };
}
