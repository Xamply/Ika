export function installHomeInstancesAutohide() {
  const zone = document.getElementById("home-instance-switcher-zone");
  const toggle = document.getElementById("setting-ui-autohide-home-instances");
  if (!zone || !toggle) return;
  const key = "ika_ui_autohide_home_instances";
  let timer;
  let hovered = zone.matches(":hover");
  try { toggle.checked = localStorage.getItem(key) === "true"; } catch {}

  const reveal = () => {
    clearTimeout(timer);
    zone.classList.remove("instances-idle-hidden");
  };
  const schedule = () => {
    reveal();
    if (!toggle.checked || hovered || zone.contains(document.activeElement)) return;
    timer = setTimeout(() => {
      if (toggle.checked && !hovered && !zone.contains(document.activeElement)) {
        zone.classList.add("instances-idle-hidden");
      }
    }, 2000);
  };
  zone.addEventListener("pointerenter", () => { hovered = true; reveal(); });
  zone.addEventListener("pointerleave", () => { hovered = false; schedule(); });
  zone.addEventListener("focusin", reveal);
  zone.addEventListener("focusout", () => queueMicrotask(schedule));
  zone.addEventListener("pointerdown", reveal);
  toggle.addEventListener("change", () => {
    try { localStorage.setItem(key, String(toggle.checked)); } catch {}
    schedule();
  });
  // Returning to Inicio starts a fresh grace period, even if it hid in another view.
  const home = document.getElementById("view-inicio");
  if (home) new MutationObserver(() => {
    if (!home.classList.contains("hidden")) {
      hovered = zone.matches(":hover");
      schedule();
    }
  }).observe(home, { attributes: true, attributeFilter: ["class"] });
  schedule();
}
