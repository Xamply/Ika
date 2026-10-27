import { DINO_SPRITE_B64 } from "./dino_assets.js";
import { DEFAULT_THEME, THEMES, THEME_IDS } from "./theme/palette.js";

// The launcher's theme (chosen in the main window, which keeps it in localStorage), and again
// whenever it changes there.
function applyTheme() {
  let theme = null;
  try {
    theme = localStorage.getItem("ika_theme_color");
  } catch {}
  if (theme === "crimson") theme = "crimson-coral";
  if (!THEMES[theme]) theme = DEFAULT_THEME;
  for (const id of THEME_IDS) document.body.classList.toggle(`theme-${id}`, id === theme && id !== DEFAULT_THEME);
}
applyTheme();
window.addEventListener("storage", (e) => {
  if (e.key === "ika_theme_color") applyTheme();
});

// Setup Tauri IPC (invoke, listen, getCurrentWindow)
const tauriGlobal = window.__TAURI__ || {};
const core = tauriGlobal.core || {};
const event = tauriGlobal.event || {};
const windowApi = tauriGlobal.window || {};

const invoke = core.invoke || (window.__TAURI_INTERNALS__?.invoke) || (async () => {});
const listen = event.listen || (async () => () => {});
const getCurrentWindow = windowApi.getCurrentWindow || (() => null);

// DOM Elements
const logContainer = document.getElementById("console-logs-container");
const footerStatus = document.getElementById("footer-status");
const footerText = document.getElementById("footer-text");
const footerIcon = document.getElementById("footer-icon");
const footerRuntime = document.getElementById("footer-runtime");
const btnMin = document.getElementById("btn-minimize-console");
const btnClose = document.getElementById("btn-close-console");
const btnOpenFull = document.getElementById("btn-open-full-console");

// Window control handlers
btnMin?.addEventListener("click", async () => {
  const win = getCurrentWindow();
  if (win?.minimize) win.minimize();
});

btnClose?.addEventListener("click", async () => {
  const win = getCurrentWindow();
  if (win?.hide) win.hide();
  // Notify main launcher
  invoke("toggle_console_window", { visible: false }).catch(() => {});
});

btnOpenFull?.addEventListener("click", async () => {
  // Tell main launcher to open full view-consola
  invoke("open_main_console_view").catch(() => {});
});

// BroadcastChannel to receive logs and state directly from main launcher
const consoleChannel = new BroadcastChannel("ika_console_channel");

const MAX_LINES = 300;
let dinoActive = false;
let dinoAnimId = null;

function renderStatus(state, detail, runtime) {
  if (footerText) {
    if (state === "preparing") {
      footerText.textContent = "Iniciando...";
    } else if (state === "running") {
      footerText.textContent = "Ejecutando juego";
    } else if (state === "crashed" || state === "failed") {
      footerText.textContent = "Juego cerrado inesperadamente";
    } else {
      footerText.textContent = detail || "Listo para jugar";
    }
  }

  if (footerIcon) {
    if (state === "crashed" || state === "failed") {
      footerIcon.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22" fill="none">
        <circle cx="12" cy="12" r="10" stroke="#ef4444" stroke-width="2.2" />
        <line x1="12" y1="8" x2="12" y2="13" stroke="#ef4444" stroke-width="2.2" stroke-linecap="round"/>
        <circle cx="12" cy="16.5" r="1" fill="#ef4444" />
      </svg>`;
      footerIcon.style.filter = "drop-shadow(0 0 8px rgba(239, 68, 68, 0.65))";
    } else if (state === "preparing" || state === "running") {
      footerIcon.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22" fill="none">
        <circle cx="12" cy="12" r="10" stroke="#e879f9" stroke-width="2.2" stroke-dasharray="16 8" />
        <polygon points="10 8 16 12 10 16" fill="#e879f9" />
      </svg>`;
      footerIcon.style.filter = "drop-shadow(0 0 8px rgba(232, 121, 249, 0.65))";
    } else {
      footerIcon.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22" fill="none">
        <circle cx="12" cy="12" r="10" stroke="#34d399" stroke-width="2.2" />
        <polyline points="7.5 12 10.5 15 16.5 9" stroke="#34d399" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>`;
      footerIcon.style.filter = "drop-shadow(0 0 8px rgba(52, 211, 153, 0.65))";
    }
  }

  if (footerRuntime && runtime) {
    footerRuntime.textContent = runtime.startsWith("Runtime:") ? runtime : `Runtime: ${runtime}`;
  }
}

function appendLogLine(line) {
  if (!logContainer) return;
  stopDino();

  const emptyHint = logContainer.querySelector(".console-empty-hint");
  if (emptyHint) emptyHint.remove();

  const isAtBottom = logContainer.scrollHeight - logContainer.scrollTop - logContainer.clientHeight <= 40;

  const div = document.createElement("div");
  const lvl = String(line.level || "INFO").toUpperCase();
  const sev = lvl === "ERROR" || lvl === "FATAL" ? "error" : lvl === "WARN" ? "warn" : "info";
  div.className = `hcp-log-line sev-${sev}`;

  const meta = document.createElement("span");
  meta.className = "hcp-line-meta";

  if (line.ts) {
    const time = new Date(line.ts).toTimeString().split(" ")[0];
    const spTime = document.createElement("span");
    spTime.className = "hcp-time";
    spTime.textContent = time;
    meta.appendChild(spTime);
  }

  const badge = document.createElement("span");
  badge.className = `hcp-badge badge-${sev}`;
  badge.textContent = lvl;
  meta.appendChild(badge);

  const msg = document.createElement("span");
  msg.className = "hcp-msg";
  msg.textContent = line.text || "";

  div.appendChild(meta);
  div.appendChild(msg);
  logContainer.appendChild(div);

  const extra = logContainer.childElementCount - MAX_LINES;
  for (let i = 0; i < extra; i++) {
    logContainer.firstElementChild?.remove();
  }

  if (isAtBottom) {
    logContainer.scrollTop = logContainer.scrollHeight;
  }
}

function startDino() {
  if (dinoActive || !logContainer) return;
  dinoActive = true;
  logContainer.textContent = "";

  const wrap = document.createElement("div");
  wrap.className = "hcp-dino-wrap";
  const canvas = document.createElement("canvas");
  canvas.className = "hcp-dino-canvas";
  canvas.width = 600;
  canvas.height = 150;

  const hint = document.createElement("div");
  hint.className = "hcp-dino-hint";
  hint.textContent = "🎮 Presiona ESPACIO o HAZ CLICK para jugar";

  wrap.appendChild(canvas);
  wrap.appendChild(hint);
  logContainer.appendChild(wrap);

  const ctx = canvas.getContext("2d");
  const sprite = new Image();
  sprite.src = DINO_SPRITE_B64;

  let dinoY = 100;
  let dinoVy = 0;
  let isJumping = false;
  let score = 0;
  let obstacles = [];
  let frame = 0;
  let gameOver = false;

  const jump = () => {
    if (gameOver) {
      gameOver = false;
      score = 0;
      obstacles = [];
      dinoY = 100;
      dinoVy = 0;
      return;
    }
    if (!isJumping) {
      dinoVy = -9;
      isJumping = true;
    }
  };

  wrap.addEventListener("pointerdown", jump);
  window.addEventListener("keydown", (e) => {
    if (e.code === "Space" || e.code === "ArrowUp") {
      e.preventDefault();
      jump();
    }
  });

  function loop() {
    if (!dinoActive) return;
    frame++;
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Ground line
    ctx.strokeStyle = "rgba(255, 255, 255, 0.2)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(0, 125);
    ctx.lineTo(canvas.width, 125);
    ctx.stroke();

    // Physics
    dinoY += dinoVy;
    dinoVy += 0.55;
    if (dinoY >= 100) {
      dinoY = 100;
      dinoVy = 0;
      isJumping = false;
    }

    // Draw Dino
    if (sprite.complete) {
      ctx.drawImage(sprite, 76, 2, 44, 47, 50, dinoY, 44, 47);
    } else {
      ctx.fillStyle = "#a855f7";
      ctx.fillRect(50, dinoY, 30, 30);
    }

    // Obstacles
    if (frame % 120 === 0 && !gameOver) {
      obstacles.push({ x: canvas.width, w: 20, h: 35 });
    }

    for (let i = obstacles.length - 1; i >= 0; i--) {
      const obs = obstacles[i];
      if (!gameOver) obs.x -= 4;

      ctx.fillStyle = "#ef4444";
      ctx.fillRect(obs.x, 125 - obs.h, obs.w, obs.h);

      // Collision
      if (obs.x < 50 + 35 && obs.x + obs.w > 50 && dinoY + 40 > 125 - obs.h) {
        gameOver = true;
      }

      if (obs.x < -30) obstacles.splice(i, 1);
    }

    // Score
    if (!gameOver) score++;
    ctx.fillStyle = "#f8fafc";
    ctx.font = "bold 13px ui-monospace, monospace";
    ctx.fillText(`SCORE: ${Math.floor(score / 5)}`, canvas.width - 110, 25);

    if (gameOver) {
      ctx.fillStyle = "rgba(0, 0, 0, 0.6)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#f87171";
      ctx.font = "bold 18px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillText("GAME OVER - CLICK PARA REINICIAR", canvas.width / 2, 75);
      ctx.textAlign = "left";
    }

    dinoAnimId = requestAnimationFrame(loop);
  }

  loop();
}

function stopDino() {
  dinoActive = false;
  if (dinoAnimId) {
    cancelAnimationFrame(dinoAnimId);
    dinoAnimId = null;
  }
}

// Listen to Channel messages
consoleChannel.onmessage = (ev) => {
  const msg = ev.data;
  if (!msg) return;

  if (msg.type === "init" || msg.type === "status") {
    renderStatus(msg.state, msg.detail, msg.runtime);
    if (msg.logs && Array.isArray(msg.logs)) {
      if (!msg.logs.length) {
        startDino();
      } else {
        logContainer.textContent = "";
        msg.logs.forEach(appendLogLine);
      }
    }
  } else if (msg.type === "log") {
    if (msg.line) appendLogLine(msg.line);
  } else if (msg.type === "clear") {
    logContainer.textContent = "";
    startDino();
  }
};

// Also listen to raw Tauri mc-game-log if available
listen("mc-game-log", (ev) => {
  const p = ev.payload || {};
  const lines = Array.isArray(p.lines) ? p.lines : [];
  lines.forEach((l) => appendLogLine(l));
}).catch(() => {});

// Signal ready to main window
consoleChannel.postMessage({ type: "ready" });
