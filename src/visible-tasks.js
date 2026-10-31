// Decode expensive media near the viewport, with a bounded number of live decoders.
export function createVisibleTaskQueue(concurrency = 2) {
  const waiting = new Map();
  const ready = [];
  let running = 0;
  let cleanupTimer = 0;
  const pump = () => {
    while (running < concurrency && ready.length) {
      const { element, task } = ready.shift();
      if (!element.isConnected) continue;
      running++;
      Promise.resolve().then(task).catch((error) => console.warn("Media preview:", error)).finally(() => {
        running--;
        pump();
      });
    }
  };
  const observer = typeof IntersectionObserver === "undefined" ? null : new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const task = waiting.get(entry.target);
      waiting.delete(entry.target);
      observer.unobserve(entry.target);
      if (task) ready.push({ element: entry.target, task });
    }
    pump();
  }, { rootMargin: "200px" });
  return (element, task) => {
    if (!observer) {
      ready.push({ element, task });
      pump();
      return;
    }
    waiting.set(element, task);
    observer.observe(element);
    if (!cleanupTimer) cleanupTimer = setTimeout(() => {
      cleanupTimer = 0;
      for (const [target] of waiting) {
        if (!target.isConnected) {
          waiting.delete(target);
          observer.unobserve(target);
        }
      }
    }, 0);
  };
}
