// Acerca de Ika Launcher: in memory of Laika. Ika carries her name (la-IKA).
//
// Each time the section opens the cinematic plays from the start: a warm night sky comes up, a
// shooting star falls and becomes her star, her portrait appears under it, her name is written
// and "Ika" stays lit in it, and the words follow one by one. Her steps cross the bottom now and
// then. A click shows everything at once. The words and timings are in style.css (.laika-tribute);
// this draws the sky (stars, the shooting star, golden motes rising) and runs the sequence.
import { onTick } from "./frame-clock.js";

const STAR_AT = { x: 0.5, y: 0.09 }; // her star (.lk-star in style.css), as a fraction of the window
const FALL = { from: 0.7, to: 1.9 }; // seconds: the shooting star
const NAMED_AT = 5.2; // seconds: "Ika" lights up in her name
const PHOTO_EVERY = 9000; // ms between her two photos

function createSky(canvas) {
  const ctx = canvas.getContext("2d");
  let w = 0;
  let h = 0;
  let dpr = 1;
  let stars = [];
  let motes = [];
  const rnd = (a, b) => a + Math.random() * (b - a);

  function resize() {
    const r = canvas.getBoundingClientRect();
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    w = r.width;
    h = r.height;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    // more stars up high, fewer near the horizon
    stars = Array.from({ length: Math.round((w * h) / 2600) }, () => {
      const y = Math.pow(Math.random(), 1.6) * h;
      return {
        x: Math.random() * w,
        y,
        r: rnd(0.25, 1.15),
        a: rnd(0.25, 0.85) * (1 - (y / h) * 0.6),
        tw: rnd(0.4, 1.6),
        ph: rnd(0, Math.PI * 2),
        gold: Math.random() < 0.3,
      };
    });
    motes = Array.from({ length: 22 }, () => newMote(true));
  }

  function newMote(anywhere) {
    return {
      x: rnd(0.08, 0.92) * w,
      y: anywhere ? rnd(0.35, 1.05) * h : h + rnd(4, 40),
      r: rnd(0.8, 2),
      v: rnd(7, 16),
      sway: rnd(6, 16),
      ph: rnd(0, Math.PI * 2),
      life: rnd(0.55, 0.85),
    };
  }

  // t: seconds since the section opened; dt: since the last frame; skip: the cinematic was skipped
  function draw(t, dt, skip) {
    if (!w) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const fadeIn = skip ? 1 : Math.min(1, t / 1.6);

    for (const s of stars) {
      const tw = 0.55 + 0.45 * Math.sin(t * s.tw + s.ph);
      ctx.globalAlpha = s.a * tw * fadeIn;
      ctx.fillStyle = s.gold ? "#ffe2b0" : "#fff8ee";
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
      ctx.fill();
    }

    // golden motes, rising slowly and fading as they go up
    for (let i = 0; i < motes.length; i++) {
      const m = motes[i];
      m.y -= m.v * dt;
      const up = 1 - m.y / h;
      if (up > m.life) {
        motes[i] = newMote(false);
        continue;
      }
      const a = Math.min(1, (1 - up / m.life) * 1.4) * Math.min(1, up * 4) * 0.7 * fadeIn;
      const x = m.x + Math.sin(t * 0.6 + m.ph) * m.sway;
      const g = ctx.createRadialGradient(x, m.y, 0, x, m.y, m.r * 4);
      g.addColorStop(0, `rgba(255, 217, 154, ${a})`);
      g.addColorStop(1, "rgba(255, 217, 154, 0)");
      ctx.globalAlpha = 1;
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(x, m.y, m.r * 4, 0, Math.PI * 2);
      ctx.fill();
    }

    // the shooting star that becomes hers
    if (!skip && t > FALL.from && t < FALL.to + 0.5) {
      const k = Math.min(1, (t - FALL.from) / (FALL.to - FALL.from));
      const ease = 1 - Math.pow(1 - k, 3);
      const x0 = w * 0.08;
      const y0 = -h * 0.02;
      const x1 = w * STAR_AT.x;
      const y1 = h * STAR_AT.y;
      const x = x0 + (x1 - x0) * ease;
      const y = y0 + (y1 - y0) * ease;
      const ang = Math.atan2(y1 - y0, x1 - x0);
      const len = 150 * (1 - k * 0.7);
      const fade = t > FALL.to ? 1 - (t - FALL.to) / 0.5 : 1;
      const tail = ctx.createLinearGradient(x, y, x - Math.cos(ang) * len, y - Math.sin(ang) * len);
      tail.addColorStop(0, `rgba(255, 240, 210, ${0.95 * fade})`);
      tail.addColorStop(1, "rgba(255, 217, 154, 0)");
      ctx.globalAlpha = 1;
      ctx.strokeStyle = tail;
      ctx.lineWidth = 1.6;
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x - Math.cos(ang) * len, y - Math.sin(ang) * len);
      ctx.stroke();
      const head = ctx.createRadialGradient(x, y, 0, x, y, 9);
      head.addColorStop(0, `rgba(255, 246, 230, ${fade})`);
      head.addColorStop(1, "rgba(255, 217, 154, 0)");
      ctx.fillStyle = head;
      ctx.beginPath();
      ctx.arc(x, y, 9, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  return { resize, draw };
}

export function createLaikaTribute({ onBack, onClose }) {
  const layer = document.getElementById("laika-tribute");
  if (!layer) return { open() {}, close() {} };
  const canvas = layer.querySelector(".lk-stars");
  const photos = [...layer.querySelectorAll(".lk-photo")];
  const sky = createSky(canvas);
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const version = document.getElementById("laika-version");
  // eslint-disable-next-line no-undef
  if (version) version.textContent = `Ika Launcher · v${typeof __IKA_VERSION__ === "string" ? __IKA_VERSION__ : ""}`;

  let untick = null;
  let started = 0;
  let last = 0;
  let skipped = false;
  const timers = [];
  let photoTimer = 0;
  const resizeObs = new ResizeObserver(() => {
    if (!layer.hidden) sky.resize();
  });
  resizeObs.observe(layer);

  function showPhoto(i) {
    photos.forEach((p, n) => p.classList.toggle("is-on", n === i));
  }

  function showAll() {
    if (skipped) return;
    skipped = true;
    layer.classList.add("is-shown", "is-named");
  }

  function open() {
    close();
    layer.hidden = false;
    layer.classList.remove("is-playing", "is-shown", "is-named");
    void layer.offsetWidth; // restart the CSS animations
    skipped = false;
    sky.resize();
    showPhoto(0);
    if (reduced) showAll();
    else {
      layer.classList.add("is-playing");
      timers.push(setTimeout(() => layer.classList.add("is-named"), NAMED_AT * 1000));
    }
    let n = 0;
    photoTimer = setInterval(() => showPhoto(++n % photos.length), PHOTO_EVERY);
    started = performance.now();
    last = started;
    untick = onTick((now) => {
      if (layer.hidden || !layer.isConnected || !layer.offsetParent) return;
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      sky.draw((now - started) / 1000, dt, skipped);
    });
  }

  function close() {
    untick?.();
    untick = null;
    timers.splice(0).forEach(clearTimeout);
    clearInterval(photoTimer);
    layer.hidden = true;
    layer.classList.remove("is-playing", "is-shown", "is-named");
  }

  layer.addEventListener("click", (e) => {
    if (e.target.closest("button")) return;
    showAll();
  });
  layer.querySelector("#btn-laika-back")?.addEventListener("click", () => {
    close();
    onBack?.();
  });
  layer.querySelector("#btn-laika-close")?.addEventListener("click", () => {
    close();
    onClose?.();
  });

  return { open, close };
}
