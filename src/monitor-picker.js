import { crtOpen, crtClose } from "./crt-fx.js";

export function installMonitorPicker(trigger, invoke) {
  const overlay = document.createElement("div");
  overlay.className = "modal-overlay hidden monitor-picker-overlay";
  overlay.id = "modal-monitor-picker";
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");
  overlay.setAttribute("aria-labelledby", "monitor-picker-title");
  overlay.innerHTML = `
    <div class="modal-box monitor-picker">
    <svg class="share-modal-border-svg monitor-picker-border-svg" aria-hidden="true">
      <rect class="share-modal-hairline-core" x="1" y="1" rx="14" ry="14"
        stroke="url(#hairlineGradRight)" stroke-width="1.6" stroke-linecap="round"
        stroke-linejoin="round" vector-effect="non-scaling-stroke" fill="none" />
    </svg>
    <div class="monitor-picker-content">
    <header><h2 id="monitor-picker-title">Elegir monitor</h2>
      <button type="button" class="modal-close titlebar-square-btn titlebar-close-btn share-close-btn monitor-picker-close"
        aria-label="Cerrar selector" title="Cerrar ventana" autofocus>
        <svg width="15" height="15" viewBox="0 0 15 15" fill="none"><path d="M2 2L13 13M13 2L2 13" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
      </button></header>
    <p>Elige dónde mostrar Ika Launcher.</p>
    <div class="monitor-picker-list"></div>
    <p class="monitor-picker-status" role="status" aria-live="polite"></p>
    </div>
    </div>`;
  document.body.append(overlay);
  const picker = overlay.querySelector(".monitor-picker");
  const border = overlay.querySelector(".monitor-picker-border-svg");
  const syncBorderHeight = () => { border.style.height = `${picker.offsetHeight}px`; };
  new ResizeObserver(syncBorderHeight).observe(picker);
  const list = overlay.querySelector(".monitor-picker-list");
  const status = overlay.querySelector(".monitor-picker-status");
  const close = overlay.querySelector("header button");
  let busy = false;
  let opening = false;
  let closing = false;
  const closePicker = async () => {
    if (overlay.classList.contains("hidden") || closing) return;
    closing = true;
    overlay.classList.add("is-closing");
    try {
      await crtClose(picker);
    } finally {
      overlay.classList.add("hidden");
      overlay.classList.remove("is-closing");
      closing = false;
      trigger.focus();
    }
  };
  close.addEventListener("click", closePicker);
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) closePicker();
  });
  overlay.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closePicker();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...overlay.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])')];
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });
  const displayLabel = (monitor, index) => {
    const name = String(monitor.displayName || monitor.name || "").trim();
    return /^(?:\\\\\.\\DISPLAY|\\DISPLAY)\d+$/i.test(name) ? "" : name;
  };
  trigger.addEventListener("click", async () => {
    if (!overlay.classList.contains("hidden") || busy || opening) return;
    opening = true;
    list.replaceChildren();
    status.textContent = "Buscando monitores…";
    picker.style.setProperty("animation", "none", "important");
    try {
      const monitors = await invoke("app_window_monitors");
      status.textContent = monitors.length ? "" : "No se encontraron monitores conectados.";
      for (const [index, monitor] of monitors.entries()) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "monitor-picker-option";
        button.setAttribute("aria-current", String(monitor.current));
        const title = document.createElement("strong");
        title.textContent = `Pantalla ${index + 1}${displayLabel(monitor, index) ? ` ${displayLabel(monitor, index)}` : ""}`;
        const detail = document.createElement("span");
        detail.textContent = `${monitor.width} × ${monitor.height} · Escala ${Math.round(monitor.scale * 100)}%${monitor.primary ? " · Principal" : ""}${monitor.current ? " · Actual" : ""}`;
        button.append(title, detail);
        button.addEventListener("click", async () => {
          if (busy) return;
          busy = true;
          list.querySelectorAll("button").forEach((item) => { item.disabled = true; });
          status.textContent = "Cambiando monitor…";
          try {
            await invoke("app_window_select_monitor", { name: monitor.name, x: monitor.x, y: monitor.y });
            await closePicker();
            // Native maximize/DPI/zoom changes settle after the command returns.
            // Refresh the SVG border from the final viewport after the modal closes.
            requestAnimationFrame(() => requestAnimationFrame(() => {
              window.dispatchEvent(new Event("ika:monitor-changed"));
            }));
          } catch (error) {
            status.textContent = `No se pudo cambiar de monitor: ${error}`;
          } finally {
            busy = false;
            list.querySelectorAll("button").forEach((item) => { item.disabled = false; });
          }
        });
        list.append(button);
      }
    } catch (error) {
      status.textContent = `No se pudieron consultar monitores: ${error}`;
    } finally {
      // CRT captures fixed light-layer bounds: populate the window before measuring it.
      opening = false;
      overlay.classList.remove("hidden");
      syncBorderHeight();
      crtOpen(picker);
      close.focus({ preventScroll: true });
    }
  });
}
