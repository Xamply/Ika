// Build-time only: rasterize the legacy WebView2 dock layers, never run
// a browser in the viewer. Pass the reference checkout as argv[2] if needed.
const fs = require("node:fs");
const path = require("node:path");
const legacyMissing = () => {
  throw new Error("Pass a checkout of the legacy-webview2 branch: node <script> <path> (or set IKA_LEGACY_DIR)");
};
const ref = path.resolve(process.argv[2] || process.env.IKA_LEGACY_DIR || legacyMissing());
const { chromium } = require(require.resolve("playwright", { paths: [ref] }));
const out = path.resolve("scratch/dock-reference");
const playOnly = process.argv.includes("--play-states-only");
const html = fs.readFileSync(path.join(ref, "index.html"), "utf8");
const js = fs.readFileSync(path.join(ref, "src/main.js"), "utf8");
const css = fs.readFileSync(path.join(ref, "src/style.css"), "utf8").replace(/^@import.*$/gm, "");
const publicImage = name => "data:image/" + (name.endsWith(".jpg") ? "jpeg" : "png") +
  ";base64," + fs.readFileSync(path.join(ref, "public", name)).toString("base64");
const from = (s, start, end) => {
  const i = s.indexOf(start);
  const j = s.indexOf(end, i);
  if (i < 0 || j < 0) throw Error("Markup not found: " + start);
  return s.slice(i, j);
};
const gradientIds = ["hairlineGradLeft", "hairlineGradRight", "dockGradProfileTop",
  "dockGradProfileBottom", "dockGradTrayTop", "dockGradTrayBottom", "jugarStrokeGrad"];
const gradient = id => html.match(new RegExp('<linearGradient id="' + id + '".*?</linearGradient>', "s"))[0];
const gradData = gradientIds.map(gradient);
const filter = html.match(/<filter id="hairlineNeonGlow".*?<\/filter>/s)[0];
const clips = [...html.matchAll(/<clipPath id="dock-cover-clip(?:-mirror)?".*?<\/clipPath>/gs)].map(m => m[0]);
const footer = from(html, '<footer class="bottom-inspector', "    <!-- Modal: Confirmacion de Eliminacion de Mod")
  .replaceAll('/play-icon-cyber.png', publicImage('play-icon-cyber.png'));
const left = from(js.slice(js.indexOf("function renderUserBadge()")), "  dock.innerHTML = `", "  `;");
const leftTemplate = left.slice(left.indexOf("`") + 1);
const leftHtml = leftTemplate.replace(/[$]{coverContentHtml}/g, '<div class="cyber-profile-custom-cover"></div>')
  .replace(/[$]{escapeHtml\(badgeTitle\)}/g, "Agregar usuario")
  .replace(/[$]{escapeHtml\(isUnregistered \? 'Sin cuenta' : state.activeUser.username\)}/g, "Sin cuenta")
  .replace(/[$]{isUnregistered \? 'is-unregistered' : ''}/g, "is-unregistered")
  .replace(/[$]{isUnregistered \? `src="[$]{NO_ACCOUNT_AVATAR}"` : ''}/g, 'src="AVATAR"')
  .replace(/[$]{isUnregistered \? 'is-offline' : ''}/g, "is-offline")
  .replace(/[$]{escapeHtml\(displayName\)}[$]{!isUnregistered[^\n]*? : ""}/g, "Agregar usuario")
  .replace(/[$]{escapeHtml\(handle\)}/g, "@invitado");
if (leftHtml.includes("${")) throw Error("Unhandled dynamic profile HTML");
const avatar = js.slice(js.indexOf("const NO_ACCOUNT_AVATAR")).match(/`(<svg.*?<\/svg>)`/s)[1];
const styles = [css,
  `:root{${[...new Set([...footer.concat(leftHtml, html.slice(0, html.indexOf(footer)), css)
    .matchAll(/--ik-([\da-f]{6})\b/gi)].map(m => m[1]))].map(hex =>
      `--ik-${hex}:${hex.match(/../g).map(n => parseInt(n, 16)).join(",")};`).join("")}}`,
  `html,body{margin:0;overflow:hidden;background:transparent} #bottom-inspector{position:absolute;bottom:0;width:100%}
  .cyber-profile-custom-cover{background-image:url("${publicImage("profile-cyber-bg.jpg")}")}
  *,*::before,*::after{animation:none!important;transition:none!important}
  .btn-wallpaper-mini-preview{display:none!important}
  `
].join("\n");
const defs = ["hairlineGradLeft", "hairlineGradRight", "dockGradProfileTop", "dockGradProfileBottom",
  "dockGradTrayTop", "dockGradTrayBottom"].map(gradient).join("") + filter + clips.join("");

async function main() {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 2560, height: 340 } });
    await page.setContent("<style>" + styles + "</style><svg width='0' height='0' style='position:absolute'><defs>" +
      defs + "</defs></svg>" + footer.replace(
        '<!-- Dynamic user badge rendered via JS -->',
        '<div class="cyber-profile-dock" id="cyber-profile-dock">' +
        leftHtml.replace("AVATAR", "data:image/svg+xml," + encodeURIComponent(avatar)) + "</div>"));
    await page.evaluate(async () => {
      for (const node of document.querySelectorAll("animateTransform")) {
        const value = (node.getAttribute("from") || node.getAttribute("values")?.split(";")[0] || "0")
          .replaceAll(" ", ",");
        node.parentElement.setAttribute("gradientTransform", node.getAttribute("type") + "(" + value + ")");
        node.remove();
      }
      for (const side of ["left", "right"]) {
        const [a,b,c,d] = side === "left" ? [40,-360,-406.666667,-1106.666667] :
          [1055,1455,1501.666667,2201.666667];
        const line = `M ${a},74 L ${b},74 L ${c},34 L ${d},34`;
        for (const stroke of ["core","bloom"]) document.querySelector(`#hairline-${side}-${stroke}`).setAttribute("d", line);
        document.querySelector(`#hairline-${side}-fill`).setAttribute("d", `${line} L ${d},350 L ${a},350 Z`);
      }
      for (const [id, value] of Object.entries({
        "spec-mods-count":"— mods", "spec-ram-allocated":"—", "spec-loader-type":"—",
        "spec-java-ver":"—", "spec-shaders-count":"— shaders",
        "spec-resourcepacks-count":"— packs", "spec-playtime":"—"
      })) document.getElementById(id).textContent = value;
      await document.fonts.ready;
      const svgImages = [...new Set([...document.querySelectorAll("svg image")].map(im => im.href.baseVal))]
        .map(src => Object.assign(new Image(), { src }));
      await Promise.all([...document.images, ...svgImages].map(im => im.decode().catch(() => {})));
    });
    if (!playOnly) await page.screenshot({ path: path.join(out, "reference.png"), omitBackground: true });

    // Keep independent browser frames for comparing the native hover, rather
    // than baking the motion itself into a flipbook.
    const freeze = '*,*::before,*::after{animation:none!important;transition:none!important}';
    if (!playOnly) await page.evaluate(rule => {
      const style = document.head.querySelector('style');
      style.textContent = style.textContent.replace(rule, '');
    }, freeze);
    if (!playOnly) await page.mouse.move(1280, 201);
    for (const ms of playOnly ? [] : [120, 420, 710]) {
      await page.evaluate(time => {
        const play = document.getElementById('btn-play-game');
        for (const motion of play.getAnimations({subtree:true})) {
          motion.pause();
          motion.currentTime = time;
        }
      }, ms);
      await page.screenshot({ path: path.join(out, `play-reference-${ms}.png`), omitBackground: true,
        clip: {x:1086, y:155, width:388, height:92} });
    }
    await page.mouse.move(1, 1);
    if (!playOnly) await page.evaluate(rule => { document.head.querySelector('style').textContent += rule; }, freeze);

    const buttons = [
      ["#btn-play-export", "Exportar instancia"], ["#btn-play-folder", "Abrir carpeta de la instancia"],
      ["#btn-play-game", "Selecciona una instancia"], ["#btn-play-settings", "Editar instancia"],
      ["#btn-play-more", "Más opciones"], ["#btn-user-badge", "Sin cuenta activa"],
      ["#btn-pref-gear", "Preferencias"], ["#btn-user-friends", "Amigos"],
      ["#btn-user-community", "Grupos"], ["#btn-link-github", "GitHub"],
      ["#btn-link-discord", "Discord"], ["#btn-link-patreon", "Patreon"],
      ["#btn-quick-wallpaper", "Cambiar fondo"]
    ];
    const layout = await page.evaluate(btns => {
      const get = selector => {
        const r = document.querySelector(selector).getBoundingClientRect();
        return [r.x, r.y, r.width, r.height];
      };
      return {
        svg: get(".cyber-dock-svg"), specs: get(".instance-specs-bar"),
        left: get("#cyber-profile-dock"), right: get("#cyber-right-dock"),
        card: get("#btn-quick-wallpaper"), buttons: btns.map(([selector, label]) => [get(selector), label])
      };
    }, buttons);
    const layers = {};
    const show = async (selector, cssExtra = "") => {
      await page.evaluate(({ selector, cssExtra }) => {
        document.querySelector("#render-layer")?.remove();
        const sheet = document.createElement("style");
        sheet.id = "render-layer";
        sheet.textContent = `#bottom-inspector{visibility:hidden} ${selector}{visibility:visible} ${cssExtra}`;
        document.head.append(sheet);
      }, { selector, cssExtra });
    };
    const clip = async (name, [x, y, w, h], pad = 0) => {
      const l = Math.floor(x - pad), t = Math.floor(y - pad);
      const r = Math.ceil(x + w + pad), b = Math.min(340, Math.ceil(y + h + pad));
      const box = { x: l, y: t, width: r-l, height: b-t };
      await page.screenshot({ path: path.join(out, name + ".png"), omitBackground: true, clip: box });
      layers[name] = [l, t, r-l, b-t];
    };
    const center = [layout.svg[0], layout.svg[1], layout.svg[2], 340 - layout.svg[1]];
    const showActiveTray = () => show("#cyber-profile-dock",
      "#btn-user-badge,.cyber-profile-border-svg,.cyber-tray-border-svg,.cyber-tray-buttons{visibility:hidden!important}");
    const capturePlayStates = async () => {
      const selector = "#btn-play-game .play-mg-text-wrap > .play-state-layer";
      const overrides = `.cyber-dock-svg{filter:none}
        ${selector}{opacity:1!important;transform:none!important}`;
      const names = {
        play_text_press: `${selector}.play-state-idle > text`,
        play_text_prep: `${selector}.play-state-preparing > text`,
        play_text_run: `${selector}.play-state-running > text`,
      };
      for (const [name, sel] of Object.entries(names)) {
        await show(sel, overrides + (name === 'play_text_press'
          ? ` ${sel}{letter-spacing:1px;transform:scale(.95)}` : ''));
        await clip(name, [1150, 174, 270, 55]);
      }
      const pathPoints = await page.locator('.play-mg-laser-orbit').evaluate(el => {
        const len = el.getTotalLength(), matrix = el.getScreenCTM();
        return Array.from({length: 2400}, (_, i) => {
          const p = el.getPointAtLength(len * i / 2400).matrixTransform(matrix);
          return [p.x, p.y];
        });
      });
      fs.writeFileSync(path.join(out, 'play-state-layout.json'), JSON.stringify({
        layers: Object.fromEntries(Object.keys(names).map(name => [name, layers[name]])), pathPoints,
      }));
    };
    if (playOnly) {
      await capturePlayStates();
      return;
    }
    if (process.argv.includes("--active-tray-only")) {
      await showActiveTray();
      await clip("left_active", layout.left, 13);
      fs.writeFileSync(path.join(out, "left-active-layout.json"), JSON.stringify(layers.left_active));
      return;
    }
    await capturePlayStates();
    await show('.cyber-dock-svg');
    await clip('shadow', [300, 70, 980, 190]);
    await show(".bottom-bar-center-zone",
      ".cyber-dock-svg{filter:none} .cyber-dock-hairline,.cyber-btn,#btn-play-game{visibility:hidden!important}");
    await clip("chassis", center, 15);
    const playBox = [1070, 126, 420, 150];
    const hidePlay = ".play-base-grad-border,.play-base-shimmer-border,.play-shimmer-sweep-wrap,.play-mg-text-wrap{visibility:hidden!important} .cyber-dock-svg{filter:none}";
    await show("#btn-play-game", hidePlay);
    await clip("play", playBox);
    await show('#btn-play-game', hidePlay + ' #btn-play-game > path:nth-of-type(1),.play-base-outer-glow{visibility:hidden!important}');
    await clip('play_hot', playBox);
    await show('#btn-play-game .play-state-idle .play-main-label', '.cyber-dock-svg{filter:none} .play-shimmer-sweep-wrap .play-main-label{visibility:hidden!important}');
    await clip('play_text', playBox);
    await show('#btn-play-game .play-state-idle .play-main-label', `.cyber-dock-svg{filter:none}
      .play-shimmer-sweep-wrap .play-main-label{visibility:hidden!important}
      #btn-play-game .play-mg-text-wrap .play-main-label{fill:white;filter:none;letter-spacing:2.2px;transform:translateX(2px) scale(1.05)}`);
    await clip('play_text_hot', playBox);
    await page.evaluate(() => {
      const svg = document.querySelector('.cyber-dock-svg');
      const icon = svg.querySelector('.play-icon-main').cloneNode();
      icon.id = 'capture-play-icon';
      icon.removeAttribute('class');
      icon.setAttribute('x', '494.5');
      icon.setAttribute('y', '57');
      svg.append(icon);
      const hud = svg.querySelector('.play-mg-inner-hud').cloneNode();
      hud.id = 'capture-play-hud';
      hud.removeAttribute('class');
      hud.setAttribute('stroke', 'white');
      svg.append(hud);
    });
    await show('#capture-play-icon', '.cyber-dock-svg{filter:none}');
    await clip('play_icon', playBox);
    await show(".play-base-grad-border",
      ".play-base-grad-border{stroke:white!important} .cyber-dock-svg{filter:none}");
    await clip("play_edge", layout.buttons[2][0], 7);
    await show('.play-mg-laser-orbit', '.cyber-dock-svg{filter:none} .cyber-dock-svg #btn-play-game .play-mg-laser-orbit{opacity:1;stroke-dasharray:none}');
    await clip('play_track', playBox);
    layout.track = await page.locator('.play-mg-laser-orbit').evaluate(el => {
      const len = el.getTotalLength();
      const matrix = el.getScreenCTM();
      return Array.from({length: 1200}, (_, i) => {
        const p = el.getPointAtLength(len*i/1200).matrixTransform(matrix);
        return [p.x,p.y];
      });
    });
    await show('#capture-play-hud', '.cyber-dock-svg{filter:none}');
    await clip('play_hud', playBox);
    await page.evaluate(() => ['capture-play-icon','capture-play-hud'].forEach(id => document.getElementById(id).remove()));
    await show(".play-shimmer-sweep-wrap", ".play-shimmer-sweep-wrap{mask:none} .cyber-dock-svg{filter:none}");
    await clip("play_sheen", [1208, 168, 176, 66]);

    for (const side of ["left", "right"]) {
      const sel = side === "left" ? "#cyber-profile-dock" : "#cyber-right-dock";
      await show(sel, ".cyber-profile-border-svg,.cyber-tray-border-svg,.cyber-tray-buttons{visibility:hidden!important}");
      await clip(side, layout[side], 13);
    }
    await showActiveTray();
    await clip("left_active", layout.left, 13);
    await show("#btn-quick-wallpaper .cyber-profile-info");
    await clip("wallpaper_label", layout.card);
    await show("#btn-quick-wallpaper .cyber-profile-bg-layer",
      "#btn-quick-wallpaper .cyber-profile-bg-layer{background:white}");
    await clip("wallpaper_mask", layout.card);

    // The four contour masks share one RGBA image in the packed atlas. The
    // mirrored module can reuse them without mirroring text or button icons.
    for (const [idx, which] of ["profile", "tray"].entries()) {
      for (const [j, child] of [1, 3].entries()) {
        const paths = `#cyber-profile-dock .cyber-${which}-border-svg path:nth-child(${child}), #cyber-profile-dock .cyber-${which}-border-svg path:nth-child(${child+1})`;
        await show(paths, `${paths.replace(", ", ",\n")}{stroke:white!important}`);
        await clip(`edge${idx*2+j}`, layout.left, 13);
      }
    }
    for (let i of [0, 1, 3, 4, 6, 7, 8, 9, 10, 11]) {
      const [x,y,w,h] = layout.buttons[i][0];
      const selector = buttons[i][0];
      await show(selector, ".cyber-dock-svg{filter:none}");
      await clip(`icon${i}`, [x+w/2-25,y+h/2-25,50,50]);
      await show(selector,
        `.cyber-dock-svg{filter:none} ${selector} .cyber-icon-hover{opacity:1}
        ${selector} .cyber-hex-icon{filter:drop-shadow(0 0 2px #fff) drop-shadow(0 0 5.5px #fffd)}`);
      await clip(`hot${i}`, [x+w/2-33,y+h/2-33,66,66]);
    }
    await page.evaluate(btns => {
      for (let i = 0; i < btns.length; i++) {
        const elem = document.createElement("span");
        elem.className = "cyber-dock-tip is-visible";
        elem.id = `tip${i}`;
        elem.textContent = btns[i][1];
        Object.assign(elem.style, { position: "absolute", left: (118 + (i % 8) * 295) + "px",
          top: (60 + Math.floor(i / 8) * 72) + "px", visibility:"visible", zIndex:"999",
          opacity:"1", transform:"translateX(-50%)" });
        document.body.append(elem);
      }
    }, buttons);
    await show("#nothing");
    layout.tipAnchors = [];
    for (let i=0; i<buttons.length; i++) {
      const box = await page.locator(`#tip${i}`).boundingBox();
      layout.tipAnchors.push([box.x + box.width/2, box.y + box.height]);
      await clip(`tip${i}`, [box.x,box.y,box.width,box.height+6], 19);
    }
    fs.writeFileSync(path.join(out, "layout.json"), JSON.stringify({ ...layout, layers }));
    fs.writeFileSync(path.join(out, "gradients.json"), JSON.stringify(gradData.map(g =>
      [...g.matchAll(/offset="([\d.]+)%" stop-color="#([\da-f]{6})"/gi)]
        .map(([,offset,rgb]) => [Number(offset)/100,rgb]))));
  } finally { await browser.close(); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
