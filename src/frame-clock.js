// One clock for the launcher's decorative animations (Gestor plates, SVG gradients, Wallpaper
// Engine glass, the idle dino). Each used to keep its own timer, so their updates landed in
// different display frames: three 30 fps animations made ~90 frames per second, and every frame
// recomposites the whole transparent window, which is most of the GPU cost while idle. On one
// clock they share each frame.
//
// The page is only woken when a tick is due, never on every refresh of the screen.

export const FRAME_MS = 1000 / 30;

const subs = new Set();
let raf = 0;
let timer = 0;
let due = 0;

function request() {
  if (!document.hidden && !raf) raf = requestAnimationFrame(run);
}

// A tick runs in the first display frame after its timer: at most one frame per tick, a few ms
// early or late (the motions follow the clock, not the frame count, so this does not show).
function run(now) {
  raf = 0;
  if (!subs.size || document.hidden) return;
  // Keep the phase: ticks every FRAME_MS, skipping the ones that were missed.
  due += FRAME_MS;
  if (due < now) due = now + FRAME_MS;
  for (const fn of [...subs]) {
    try {
      fn(now);
    } catch (err) {
      console.error(err);
    }
  }
  if (subs.size && !document.hidden && !timer) {
    timer = setTimeout(() => {
      timer = 0;
      request();
    }, Math.max(0, due - performance.now() - 2));
  }
}

document.addEventListener("visibilitychange", () => {
  clearTimeout(timer);
  cancelAnimationFrame(raf);
  timer = 0;
  raf = 0;
  if (!document.hidden && subs.size) {
    due = performance.now() - FRAME_MS;
    request();
  }
});

/** Calls `fn(now)` in the display frame of every tick until the returned function is called. */
export function onTick(fn) {
  subs.add(fn);
  if (!raf && !timer) {
    due = performance.now() - FRAME_MS;
    request();
  }
  return () => {
    subs.delete(fn);
    if (!subs.size) {
      clearTimeout(timer);
      cancelAnimationFrame(raf);
      timer = 0;
      raf = 0;
    }
  };
}

/** Resume a cached scene after its first presentation, not during section layout. */
export function onTickAfterPaint(fn) {
  let stop = null;
  let frame = requestAnimationFrame(() => {
    frame = requestAnimationFrame(() => {
      frame = 0;
      stop = onTick(fn);
    });
  });
  return () => {
    cancelAnimationFrame(frame);
    stop?.();
  };
}
