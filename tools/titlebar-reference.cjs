// Authoring/verification only. Reads the legacy WebView2 checkout; the viewer embeds
// the resulting small atlas and does not run Chromium or load HTML/CSS.
const fs = require("node:fs");
const path = require("node:path");
const legacyMissing = () => {
  throw new Error("Pass a checkout of the legacy-webview2 branch: node <script> <path> (or set IKA_LEGACY_DIR)");
};
const reference = path.resolve(process.argv[2] || process.env.IKA_LEGACY_DIR || legacyMissing());
const { chromium } = require(require.resolve("playwright", { paths: [reference] }));
const out = path.resolve("scratch/titlebar-reference");
const html = fs.readFileSync(path.join(reference, "index.html"), "utf8");
const css = fs.readFileSync(path.join(reference, "src/style.css"), "utf8").replace(/^@import.*$/gm, "");
const logo = fs.readFileSync(path.join(reference, "public/app-icon-64.png")).toString("base64");
const gradient = id => html.match(new RegExp('<linearGradient id="' + id + '".*?</linearGradient>', 's'))[0];
const gradients = ["hairlineGradRight", "dockGradTrayTop"].map(gradient);
const filter = html.match(/<filter id="hairlineNeonGlow".*?<\/filter>/s)[0];
const title = html.slice(html.indexOf('<div class="custom-titlebar"'), html.indexOf('<!-- Notification center (bell in the title bar) -->'))
  .replace('/app-icon-64.png', 'data:image/png;base64,' + logo);
const tokens = new Map();
for (const m of title.concat(gradients.join("")).matchAll(/(#[\da-f]{6})"\s+style="[^"]*var\((--ik-[\w-]+)\)/gi)) {
  const rgb = m[1].slice(1).match(/../g).map(hex => parseInt(hex, 16));
  tokens.set(m[2], rgb.join(","));
}
const defs = '<svg style="position:absolute;width:0;height:0"><defs>' +
  gradients.join("").replace(/<animateTransform\b[^>]*\/>/gs, "") + filter + '</defs></svg>';
const style = css + ":root{" + [...tokens].map(pair => pair.join(":")).join(";") + "}" +
  "body{background:#202020} #titlebar-btn-update{display:none!important} #titlebar-notch-fill{fill:#040416!important}";

async function run() {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 1152, height: 210 } });
    await page.setContent("<style>" + style + "</style>" + defs + title);
    await page.evaluate(() => {
      document.querySelector("#titlebar-notch-fill").setAttribute("d",
        "M 0,0 L 940,0 L 940,52 C 900,52 890,102 850,102 L 90,102 C 50,102 40,52 0,52 Z");
      const r = document.querySelector("#notch-shape").getBoundingClientRect();
      const start = Math.min(0, Math.floor(-r.left) - 40);
      const end = Math.max(940, Math.ceil(innerWidth - r.left) + 40);
      for (const stroke of document.querySelectorAll(".titlebar-hairline path")) {
        stroke.setAttribute("d", `M ${start},52 L 0,52 C 40,52 50,102 90,102 L 850,102 C 890,102 900,52 940,52 L ${end},52`);
      }
      const glider = document.querySelector("#view-switcher-glider");
      const element = document.querySelector(".view-btn.active");
      glider.style.transition = "none";
      glider.style.opacity = "1";
      glider.style.transform = `translateX(${element.offsetLeft}px) scaleX(${element.offsetWidth / 164})`;
    });
    const phase = t => page.evaluate(t => {
        document.querySelector("#hairlineGradRight").setAttribute("gradientTransform", `translate(${(t % 18) * 2400 / 18})`);
        document.querySelector("#dockGradTrayTop").setAttribute("gradientTransform", `translate(${1200 - (t % 25) * 2400 / 25})`);
      }, t);
    for (const seconds of [0, 4.5, 9, 13.5, 18, 25]) {
      await phase(seconds);
      await page.screenshot({ path: path.join(out, "ref-" + seconds + ".png") });
    }
    await phase(0);
    for (let i = 0; i < 5; i++) {
      await page.locator(".view-btn").nth(i).hover();
      await page.waitForTimeout(350);
      await page.screenshot({
        clip: { x: 176, y: 41, width: 800, height: 50 },
        path: path.join(out, `ref-hover-${i}.png`),
      });
    }
    await page.mouse.move(70, 12);
    await page.waitForTimeout(350);

    await page.setViewportSize({ width: 1152, height: 648 });
    const pickerSource = fs.readFileSync(path.join(reference, "src/monitor-picker.js"), "utf8")
      .replace(/^import.*$/m, "").replace("export function", "function");
    await page.addScriptTag({ content: pickerSource +
      `\nconst crtOpen = () => {}; const crtClose = () => Promise.resolve();
      installMonitorPicker(document.querySelector("#titlebar-btn-maximize"), async () =>
        [{width: 2560, height: 1440, scale: 1, current: true, primary: true}]);`
    });
    await page.locator("#titlebar-btn-maximize").click();
    await page.mouse.move(10, 200);
    await page.waitForTimeout(480);
    await page.screenshot({ path: path.join(out, "ref-monitor-settled.png") });
    const monitorLayout = await page.evaluate(() => [
      ".monitor-picker", ".monitor-picker h2", ".monitor-picker p", ".monitor-picker-option",
      ".monitor-picker-option strong", ".monitor-picker-option span", ".monitor-picker-close"
    ].map(selector => {
      const { x: left, y: top } = document.querySelector(".monitor-picker").getBoundingClientRect();
      const { x, y, width, height } = document.querySelector(selector).getBoundingClientRect();
      return [x - left, y - top, width, height];
    }));
    fs.writeFileSync(path.join(out, "monitor-layout.json"), JSON.stringify(monitorLayout));
    const layout = await page.evaluate(() => {
      const buttons = [...document.querySelectorAll(".view-btn")];
      const widths = [false, true].map(active => buttons.map(btn => {
        btn.classList.toggle("active", active);
        return btn.getBoundingClientRect().width;
      }));
      const nav = document.querySelector(".view-switcher").getBoundingClientRect();
      return { widths, navY: nav.y, navHeight: nav.height };
    });
    const sprites = await page.evaluate(() => {
      const labels = [...document.querySelectorAll(".view-btn span")].map(e => e.textContent.trim());
      const svg = selector => document.querySelector(selector).outerHTML.replace(/ class="[^"]*"/g, "");
      const assets = [
        ["logo", 28, 28, document.querySelector(".titlebar-app-icon").outerHTML],
        ["name", 88, 22, '<span style="font-weight:500;font-size:13px;letter-spacing:.2px">Ika Launcher</span>'],
      ];
      for (let i = 0; i < labels.length; i++) {
        for (let active = 0; active <= 1; active++) {
          assets.push([`text${i}_${active}`, 154, 24, `<span style="font-size:16px;font-weight:${active ? 800 : 700}">${labels[i]}</span>`]);
        }
        const dim = [18, 18, 19, 17, 18][i];
        if (i === 3) {
          [...document.querySelectorAll(".tab-icon-manager-layers > *")].forEach((element, layer) => {
            assets.push([`plate${layer}`, dim, dim, `<svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${element.outerHTML}</svg>`]);
          });
        } else {
          assets.push([`icon${i}`, dim, dim, svg(`.view-btn:nth-of-type(${i + 1}) > svg`)]);
        }
      }
      for (const [id, dim] of [["notify", 21], ["minimize", 11], ["maximize", 15], ["close", 15]]) {
        assets.push([id, dim, dim, svg(`#titlebar-btn-${id} > svg`)]);
      }
      assets.push(["notifyEmptyIcon", 28, 28,
        svg("#titlebar-btn-notify > svg").replace('stroke-width="1.9"', 'stroke-width="1.6"')]);
      for (const [name, font, label] of [
        ["notifyTitle", "font-weight:700;font-size:15.5px;letter-spacing:.2px", "Notificaciones"],
        ["notifyRead", "font-size:12.5px", "Marcar como leídas"],
        ["notifyClear", "font-size:12.5px", "Limpiar"],
        ["notifyEmpty", "font-size:14px", "No hay notificaciones ni descargas"],
      ]) {
        assets.push([name, 240, 22, `<span style="${font}">${label}</span>`]);
      }
      for (const [name, font, label] of [
        ["monitorTitle", "font-weight:700;font-size:22px", "Elegir monitor"],
        ["monitorIntro", "font-size:16px", "Elige dónde mostrar Ika Launcher."],
        ["monitorLabel", "font-weight:700;font-size:16px", "Pantalla"],
        ["monitorTimes", "font-size:13px", "×"],
        ["monitorScale", "font-size:13px", "· Escala"],
        ["monitorPercent", "font-size:13px", "%"],
        ["monitorPrimary", "font-size:13px", "· Principal"],
        ["monitorCurrent", "font-size:13px", "· Actual"],
        ["monitorEmpty", "font-size:16px", "No se encontraron monitores conectados."],
        ["monitorError", "font-size:16px", "Monitor desconectado. Abre el selector de nuevo."],
      ]) {
        assets.push([name, 0, name === "monitorTitle" ? 28 : 20, `<span style="${font}">${label}</span>`]);
      }
      for (let digit = 0; digit <= 9; digit++) {
        assets.push([`digit${digit}`, 0, 18, `<span style="font-size:13px">${digit}</span>`]);
        assets.push([`bold${digit}`, 0, 20, `<span style="font-weight:700;font-size:16px">${digit}</span>`]);
      }
      return assets;
    });
    const atlas = await browser.newPage({ viewport: { width: 300, height: 420 }, deviceScaleFactor: 2 });
    await atlas.setContent(`<style>*{box-sizing:border-box}body{margin:0;color:white;
      font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}
      .sprite{position:absolute;display:flex;align-items:center;white-space:nowrap}
      svg,img{display:block;width:100%;height:100%}</style>`);
    const packed = await atlas.evaluate(items => {
      const spec = {};
      let left = 1, y = 6, row = 0;
      for (let [name, width, height, markup] of items) {
        const sprite = document.createElement("div");
        sprite.className = "sprite";
        sprite.style.height = height + "px";
        sprite.innerHTML = markup;
        document.body.append(sprite);
        if (sprite.firstElementChild.tagName === "SPAN") {
          width = Math.ceil(sprite.firstElementChild.getBoundingClientRect().width) + 2;
          if (width > 298) {
            sprite.style.whiteSpace = "normal";
            width = 296;
            height *= 2;
          }
        }
        if (left + width > 299) { left = 1; y += row + 2; row = 0; }
        Object.assign(sprite.style, { left: left + "px", top: y + "px", width: width + "px" });
        spec[name] = [left, y, width, height];
        left += width + 2;
        row = Math.max(row, height);
      }
      return { rects: spec, height: y + row + 1 };
    }, sprites);
    if (packed.height > 420) throw new Error("Titlebar atlas exceeded its viewport");
    await atlas.screenshot({ path: path.join(out, "sprites.png"), omitBackground: true,
      clip: { x: 0, y: 0, width: 300, height: packed.height } });
    fs.writeFileSync(path.join(out, "layout.json"), JSON.stringify({ ...layout, ...packed }, null, 2));
    fs.writeFileSync(path.join(out, "gradients.json"), JSON.stringify(gradients.map(g =>
      [...g.matchAll(/offset="([\d.]+)%" stop-color="#([\da-f]{6})"/gi)]
        .map(([, offset, rgb]) => [Number(offset) / 100, rgb])), null, 2));
  } finally {
    await browser.close();
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
