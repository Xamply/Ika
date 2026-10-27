// A cathode-ray-tube power on / power off for a window (Wallpapers, Exportar instancia).
//
// On: a dot of light appears in the middle of the window, stretches into a glowing horizontal
// line, the line splits into two beams that run to the top and bottom edges while the picture
// opens behind them in a white phosphor flash, with tube scanlines that fade as it settles.
// Off: the picture collapses into a bright line, the line shrinks to a dot, the dot flares and
// dies out.
//
// The light is drawn in its own layer over the window (not inside it: the window clips its
// contents, which left thin dark edges around the flash). Transform and opacity only.

const reduceMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// Where the window really is: its laid-out size around its on-screen centre (another entrance
// animation may be scaling it at this very moment, which made the light smaller than the window).
function layoutRect(box) {
  const r = box.getBoundingClientRect();
  const w = box.offsetWidth || r.width;
  const h = box.offsetHeight || r.height;
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;
  return { left: cx - w / 2, top: cy - h / 2, width: w, height: h };
}

function fxLayer(rect, radius, off = false) {
  const root = document.createElement("div");
  root.className = off ? "crt-fx crt-fx-off" : "crt-fx";
  root.style.cssText = `left:${rect.left}px;top:${rect.top}px;width:${rect.width}px;height:${rect.height}px;--crt-radius:${radius}`;
  root.innerHTML = `
    <div class="crt-fx-flash"></div>
    <div class="crt-fx-scan"></div>
    <div class="crt-fx-beam crt-fx-beam-a"></div>
    <div class="crt-fx-beam crt-fx-beam-b"></div>
    <div class="crt-fx-dot"></div>`;
  document.body.appendChild(root);
  return root;
}

const part = (root, name) => root.querySelector(`.crt-fx-${name}`);

/** Power-on animation for `box` (already laid out where it will stay). */
function tubeOpen(box, duration = 760) {
  if (!box || reduceMotion()) return;
  box.getAnimations().forEach((a) => a.id === "crt" && a.cancel());
  const rect = layoutRect(box);
  if (rect.width < 4 || rect.height < 4) return;
  const radius = getComputedStyle(box).borderRadius || "16px";
  const fx = fxLayer(rect, radius);
  const ease = "cubic-bezier(0.22, 1, 0.36, 1)";
  const opts = (extra = {}) => ({ duration, easing: "linear", fill: "both", ...extra });
  // the picture: a dot, a line, then the full height with a small vertical bounce
  const screen = [
    { offset: 0, opacity: 0, transform: "scale(0.004, 0.003)" },
    { offset: 0.16, opacity: 0, transform: "scale(0.004, 0.003)" },
    { offset: 0.17, opacity: 1, transform: "scale(0.01, 0.003)", easing: ease },
    { offset: 0.42, opacity: 1, transform: "scale(1, 0.003)", easing: ease },
    { offset: 0.74, opacity: 1, transform: "scale(1, 1.035)", easing: "ease-in-out" },
    { offset: 0.86, opacity: 1, transform: "scale(1, 0.988)", easing: "ease-in-out" },
    { offset: 1, opacity: 1, transform: "scale(1, 1)" },
  ];
  const a = box.animate(screen, opts());
  a.id = "crt";
  // The flash and the scanlines grow exactly like the picture (same transform keyframes), so
  // the window's own edges never show past them; only their opacity has its own timing.
  const sameShape = screen.map(({ offset, transform, easing }) => (easing ? { offset, transform, easing } : { offset, transform }));
  const flash = part(fx, "flash");
  flash.animate(sameShape, opts());
  flash.animate(
    [
      { offset: 0, opacity: 0 },
      { offset: 0.16, opacity: 0 },
      { offset: 0.17, opacity: 1 },
      { offset: 0.46, opacity: 1 },
      { offset: 0.74, opacity: 0.4 },
      { offset: 0.9, opacity: 0.06 },
      { offset: 1, opacity: 0 },
    ],
    opts()
  );
  const scan = part(fx, "scan");
  scan.animate(sameShape, opts());
  // only once the picture is (almost) at its full height: thin lines squeezed by the unfold broke
  // into uneven bands
  scan.animate(
    [
      { offset: 0, opacity: 0 },
      { offset: 0.7, opacity: 0 },
      { offset: 0.78, opacity: 0.8 },
      { offset: 0.9, opacity: 0.4 },
      { offset: 1, opacity: 0 },
    ],
    opts()
  );
  part(fx, "dot").animate(
    [
      { offset: 0, opacity: 0, transform: "scale(0.1)" },
      { offset: 0.08, opacity: 1, transform: "scale(1.4)", easing: ease },
      { offset: 0.17, opacity: 1, transform: "scale(1)" },
      { offset: 0.3, opacity: 0.6, transform: "scale(2.2, 0.6)" },
      { offset: 0.42, opacity: 0, transform: "scale(4, 0.3)" },
      { offset: 1, opacity: 0, transform: "scale(4, 0.3)" },
    ],
    opts()
  );
  // the line, then two beams running to the top and bottom edges
  const half = rect.height / 2;
  for (const [name, dir] of [["beam-a", -1], ["beam-b", 1]]) {
    part(fx, name).animate(
      [
        { offset: 0, opacity: 0, transform: "translateY(0) scaleX(0.004)" },
        { offset: 0.16, opacity: 0, transform: "translateY(0) scaleX(0.004)" },
        { offset: 0.17, opacity: 1, transform: "translateY(0) scaleX(0.01)", easing: ease },
        { offset: 0.42, opacity: 1, transform: "translateY(0) scaleX(1)", easing: "ease-out" },
        // they fade halfway, never reaching the window's top and bottom edges
        { offset: 0.6, opacity: 0, transform: `translateY(${dir * half * 0.55}px) scaleX(1)` },
        { offset: 1, opacity: 0, transform: `translateY(${dir * half * 0.55}px) scaleX(1)` },
      ],
      opts()
    );
  }
  // once settled, the window goes back to its own styles (the animation no longer holds it)
  a.finished.then(() => a.cancel(), () => {}).finally(() => fx.remove());
  return a;
}

/** Power-off animation for `box`; resolves when it is over. */
function tubeClose(box, duration = 420) {
  if (!box || reduceMotion()) return Promise.resolve();
  box.getAnimations().forEach((x) => x.id === "crt" && x.cancel());
  const rect = layoutRect(box);
  if (rect.width < 4 || rect.height < 4) return Promise.resolve();
  const radius = getComputedStyle(box).borderRadius || "16px";
  const fx = fxLayer(rect, radius, true);
  const opts = { duration, easing: "linear", fill: "both" };
  const squeeze = "cubic-bezier(0.55, 0, 0.75, 0.2)";
  const a = box.animate(
    [
      { offset: 0, opacity: 1, transform: "scale(1, 1)", easing: squeeze },
      { offset: 0.42, opacity: 1, transform: "scale(1.01, 0.003)", easing: squeeze },
      { offset: 0.72, opacity: 1, transform: "scale(0.004, 0.003)" },
      { offset: 0.73, opacity: 0, transform: "scale(0.004, 0.003)" },
      { offset: 1, opacity: 0, transform: "scale(0.004, 0.003)" },
    ],
    opts
  );
  a.id = "crt";
  part(fx, "flash").animate(
    [
      { offset: 0, opacity: 0, transform: "scale(1, 1)", easing: squeeze },
      { offset: 0.3, opacity: 0.75, transform: "scale(1, 0.3)", easing: squeeze },
      { offset: 0.42, opacity: 1, transform: "scale(1.01, 0.003)", easing: squeeze },
      { offset: 0.72, opacity: 1, transform: "scale(0.004, 0.003)" },
      { offset: 0.73, opacity: 0, transform: "scale(0.004, 0.003)" },
      { offset: 1, opacity: 0, transform: "scale(0.004, 0.003)" },
    ],
    opts
  );
  for (const name of ["beam-a", "beam-b"]) {
    part(fx, name).animate(
      [
        { offset: 0, opacity: 0, transform: "translateY(0) scaleX(1)" },
        { offset: 0.38, opacity: 0, transform: "translateY(0) scaleX(1)" },
        { offset: 0.42, opacity: 1, transform: "translateY(0) scaleX(1)", easing: squeeze },
        { offset: 0.72, opacity: 1, transform: "translateY(0) scaleX(0.004)" },
        { offset: 0.73, opacity: 0, transform: "translateY(0) scaleX(0.004)" },
        { offset: 1, opacity: 0, transform: "translateY(0) scaleX(0.004)" },
      ],
      opts
    );
  }
  part(fx, "dot").animate(
    [
      { offset: 0, opacity: 0, transform: "scale(1)" },
      { offset: 0.66, opacity: 0, transform: "scale(1)" },
      { offset: 0.74, opacity: 1, transform: "scale(1.5)" },
      { offset: 0.86, opacity: 0.9, transform: "scale(0.8)" },
      { offset: 1, opacity: 0, transform: "scale(0.05)" },
    ],
    opts
  );
  return a.finished.catch(() => {}).finally(() => fx.remove());
}

// ---------------------------------------------------------------------------------------------
// Every window of the launcher (.modal-overlay) switches on and off like this.
//
// Opening: when a window stops being hidden, its own entrance animation is turned off and the
// power-on plays on its box. Closing: windows hide in many different ways (most at once, some
// after a closing phase of their own, `.is-closing`), so the power-off plays on a copy of the
// window drawn where it was, while the real one is already closed: its logic is untouched (it can
// be reopened at once, its state is the real one).

const CLOSE_MS = 420;

function boxOf(overlay) {
  return overlay.querySelector(":scope > .modal-box") || overlay.querySelector(".modal-box");
}

function quietOwnAnimation(box) {
  // inline !important: some windows set theirs with !important rules of their own
  box.style.setProperty("animation", "none", "important");
}

function playCopyClosing(overlay) {
  const box = boxOf(overlay);
  if (!box || reduceMotion() || getWindowAnim() === "none") return;
  const copy = overlay.cloneNode(true);
  copy.classList.remove("hidden", "is-closing", "is-closing-close", "is-closing-cancel");
  copy.removeAttribute("data-crt-closing");
  copy.style.visibility = "";
  copy.style.pointerEvents = "none";
  copy.setAttribute("aria-hidden", "true");
  copy.dataset.crtCopy = "1";
  // the copy is a closing window: none of the window's own CSS animations (its entrance: the
  // backdrop fading in, the contents coming in one by one) may run on it
  copy.classList.add("crt-copy");
  // no media starting to load in the copy; canvases keep their picture
  copy.querySelectorAll("video, iframe, audio").forEach((e) => e.remove());
  const srcCanvases = overlay.querySelectorAll("canvas");
  copy.querySelectorAll("canvas").forEach((c, i) => {
    const src = srcCanvases[i];
    if (!src || !src.width || !src.height) return;
    try {
      c.width = src.width;
      c.height = src.height;
      c.getContext("2d")?.drawImage(src, 0, 0);
    } catch (_) {}
  });
  const copyBox = boxOf(copy);
  if (!copyBox) return;
  // laid out exactly where the real box was
  const rect = layoutRect(box);
  quietOwnAnimation(copyBox);
  document.body.appendChild(copy);
  // as Wallpapers closes: the backdrop stays until halfway, then fades; the contents go first,
  // rising a little, and the window's own closing takes over
  const backdrop = copy.animate([{ opacity: 1 }, { opacity: 1, offset: 0.55 }, { opacity: 0 }], { duration: CLOSE_MS, easing: "cubic-bezier(0.4, 0, 0.2, 1)", fill: "both" });
  Array.from(copyBox.children).forEach((child) => {
    if (child.classList?.contains("crt-fx")) return;
    child.animate([{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateY(-6px)" }], { duration: 140, easing: "ease-in", fill: "both" });
  });
  const r2 = layoutRect(copyBox);
  if (Math.abs(r2.left - rect.left) > 1 || Math.abs(r2.top - rect.top) > 1) {
    copyBox.style.translate = `${rect.left - r2.left}px ${rect.top - r2.top}px`;
  }
  Promise.all([crtClose(copyBox, CLOSE_MS), backdrop.finished.catch(() => {})]).finally(() => copy.remove());
}

export function installCrtModals({ exclude = [] } = {}) {
  const skip = new Set(exclude);
  const watched = new WeakSet();
  const onClassChange = (records) => {
    for (const rec of records) {
      const ov = rec.target;
      if (!(ov instanceof HTMLElement) || skip.has(ov.id) || ov.dataset.crtCopy) continue;
      const before = new Set((rec.oldValue || "").split(/\s+/));
      const wasHidden = before.has("hidden");
      const isHidden = ov.classList.contains("hidden");
      const closingNow = ov.classList.contains("is-closing") && !before.has("is-closing");
      if (wasHidden && !isHidden) {
        // opened (again): the real window shows, with the power-on
        delete ov.dataset.crtClosing;
        ov.style.visibility = "";
        const box = boxOf(ov);
        if (box) {
          quietOwnAnimation(box);
          crtOpen(box);
        }
      } else if (!isHidden && closingNow && !ov.dataset.crtClosing) {
        // a window with a closing phase of its own: the power-off starts now, on the copy
        ov.dataset.crtClosing = "1";
        playCopyClosing(ov);
        ov.style.visibility = "hidden";
      } else if (!wasHidden && isHidden) {
        if (ov.dataset.crtClosing) {
          delete ov.dataset.crtClosing;
          ov.style.visibility = "";
        } else {
          // hidden at once: rebuild it for a moment to copy it, then play the power-off
          ov.classList.remove("hidden");
          playCopyClosingSync(ov);
        }
      }
    }
  };
  // Called with the window just hidden: its layout is read with it shown for an instant (same
  // task, nothing is painted in between) and hidden again before the copy plays.
  function playCopyClosingSync(ov) {
    observer.disconnect();
    try {
      playCopyClosing(ov);
    } finally {
      ov.classList.add("hidden");
      watchAll();
    }
  }
  const observer = new MutationObserver(onClassChange);
  const watch = (ov) => {
    observer.observe(ov, { attributes: true, attributeFilter: ["class"], attributeOldValue: true });
    watched.add(ov);
  };
  function watchAll() {
    document.querySelectorAll(".modal-overlay:not([data-crt-copy])").forEach(watch);
  }
  watchAll();
  // windows created later
  new MutationObserver((records) => {
    for (const rec of records) {
      rec.addedNodes.forEach((n) => {
        if (n instanceof HTMLElement && n.classList.contains("modal-overlay") && !watched.has(n) && !n.dataset.crtCopy) watch(n);
      });
    }
  }).observe(document.body, { childList: true });
}

// ---------------------------------------------------------------------------------------------
// The windows' animation is a choice (Aspecto visual > Inicio): the tube above, or one of these.
// Every window opens and closes through crtOpen / crtClose, so the choice reaches all of them.
// ---------------------------------------------------------------------------------------------
const ANIM_KEY = "ika_window_anim";
export const WINDOW_ANIMS = [
  ["crt", "Tubo CRT"],
  ["soft", "Suave"],
  ["slide", "Deslizar"],
  ["flip", "Despliegue 3D"],
  ["holo", "Holograma"],
  ["portal", "Portal"],
  ["none", "Ninguna"],
];
export function getWindowAnim() {
  try {
    const v = localStorage.getItem(ANIM_KEY);
    return WINDOW_ANIMS.some(([id]) => id === v) ? v : "crt";
  } catch {
    return "crt";
  }
}
export function setWindowAnim(id) {
  try {
    localStorage.setItem(ANIM_KEY, id);
  } catch {}
}

const SPRING = "cubic-bezier(0.16, 1, 0.3, 1)";
const RGB = (x) => `drop-shadow(${-x}px 0 0 rgba(255, 0, 90, 0.75)) drop-shadow(${x}px 0 0 rgba(0, 235, 255, 0.75))`;
const STYLES = {
  soft: {
    open: [[{ opacity: 0, transform: "translateY(8px) scale(0.96)" }, { opacity: 1, transform: "none" }], 380, SPRING],
    close: [[{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateY(4px) scale(0.97)" }], 260, "ease-in"],
  },
  slide: {
    open: [[{ opacity: 0, transform: "translateY(48px)", filter: "blur(8px)" }, { opacity: 1, transform: "none", filter: "blur(0)" }], 520, SPRING],
    close: [[{ opacity: 1, transform: "none", filter: "blur(0)" }, { opacity: 0, transform: "translateY(36px)", filter: "blur(6px)" }], 300, "cubic-bezier(0.4, 0, 1, 1)"],
  },
  flip: {
    open: [
      [
        { opacity: 0, transform: "perspective(1400px) rotateX(-22deg) translateY(-18px) scale(0.94)", transformOrigin: "50% 0" },
        { opacity: 1, transform: "perspective(1400px) rotateX(4deg) scale(1.005)", offset: 0.7, transformOrigin: "50% 0" },
        { opacity: 1, transform: "none", transformOrigin: "50% 0" },
      ],
      560,
      "cubic-bezier(0.2, 0.9, 0.3, 1)",
    ],
    close: [[{ opacity: 1, transform: "none", transformOrigin: "50% 0" }, { opacity: 0, transform: "perspective(1400px) rotateX(16deg) scale(0.95)", transformOrigin: "50% 0" }], 320, "ease-in"],
  },
  holo: {
    open: [
      [
        { opacity: 0, filter: RGB(10), clipPath: "inset(48% 0 48% 0)", transform: "translateX(-6px)" },
        { opacity: 1, filter: RGB(8), clipPath: "inset(30% 0 52% 0)", transform: "translateX(8px)", offset: 0.15 },
        { opacity: 0.4, filter: RGB(6), clipPath: "inset(10% 0 20% 0)", transform: "translateX(-4px)", offset: 0.3 },
        { opacity: 1, filter: RGB(5), clipPath: "inset(0 0 60% 0)", transform: "translateX(3px)", offset: 0.42 },
        { opacity: 0.7, filter: RGB(4), clipPath: "inset(0 0 0 0)", transform: "translateX(-2px)", offset: 0.56 },
        { opacity: 1, filter: RGB(2), clipPath: "inset(0 0 0 0)", transform: "translateX(1px)", offset: 0.75 },
        { opacity: 1, filter: RGB(0), clipPath: "inset(0 0 0 0)", transform: "none" },
      ],
      560,
      "steps(14, end)",
    ],
    close: [
      [
        { opacity: 1, filter: RGB(0), clipPath: "inset(0 0 0 0)", transform: "none" },
        { opacity: 0.6, filter: RGB(5), clipPath: "inset(20% 0 10% 0)", transform: "translateX(6px)", offset: 0.35 },
        { opacity: 1, filter: RGB(8), clipPath: "inset(45% 0 30% 0)", transform: "translateX(-8px)", offset: 0.6 },
        { opacity: 0, filter: RGB(10), clipPath: "inset(50% 0 50% 0)", transform: "none" },
      ],
      340,
      "steps(10, end)",
    ],
  },
  portal: {
    open: [
      [
        { opacity: 0, transform: "scale(0.62)", filter: "blur(14px) brightness(1.6)" },
        { opacity: 1, transform: "scale(1.025)", filter: "blur(0) brightness(1.1)", offset: 0.72 },
        { opacity: 1, transform: "none", filter: "blur(0) brightness(1)" },
      ],
      560,
      SPRING,
    ],
    close: [[{ opacity: 1, transform: "none", filter: "blur(0)" }, { opacity: 0, transform: "scale(0.7)", filter: "blur(12px) brightness(1.5)" }], 320, "cubic-bezier(0.5, 0, 0.75, 0)"],
  },
};

function play(box, spec) {
  box.getAnimations().forEach((a) => a.id === "crt" && a.cancel());
  const [frames, duration, easing] = spec;
  const a = box.animate(frames, { duration, easing, fill: "both" });
  a.id = "crt";
  return a;
}

/** The chosen opening animation for `box` (already laid out where it will stay). */
export function crtOpen(box, duration = 760) {
  if (!box || reduceMotion()) return;
  const style = getWindowAnim();
  if (style === "none") return;
  if (style === "crt" || !STYLES[style]) return tubeOpen(box, duration);
  const a = play(box, STYLES[style].open);
  // nothing of it left once in place (fill would otherwise keep a transform on the window)
  a.finished.then(() => a.cancel()).catch(() => {});
}

/** The chosen closing animation for `box`; resolves when it is over. */
export function crtClose(box, duration = 420) {
  if (!box || reduceMotion()) return Promise.resolve();
  const style = getWindowAnim();
  if (style === "none") return Promise.resolve();
  if (style === "crt" || !STYLES[style]) return tubeClose(box, duration);
  return play(box, STYLES[style].close).finished.catch(() => {});
}
