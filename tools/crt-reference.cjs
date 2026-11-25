// Read the WebView2 reference with a clean Edge page; do not launch or edit it.
const fs = require("node:fs");
const path = require("node:path");
const legacyMissing = () => {
  throw new Error("Pass a checkout of the legacy-webview2 branch: node <script> <path> (or set IKA_LEGACY_DIR)");
};
const reference = path.resolve(process.argv[2] || process.env.IKA_LEGACY_DIR || legacyMissing());
const out = path.resolve("scratch/crt-reference");
const { chromium } = require(require.resolve("playwright", { paths: [reference] }));
const src = (file) => fs.readFileSync(path.join(reference, "src", file), "utf8");
const css = src("style.css");
const first = css.indexOf(".crt-fx {");
const last = css.indexOf("/* Exportar instancia: its glowing border", first);
if (first < 0 || last < first) throw new Error("Legacy CRT styles not found");
const fx = css.slice(first, last);
const animations = src("crt-fx.js").split("// Every window of the launcher")[0].replace(/^import[^\n]*\n/, "");

async function main() {
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch({ channel: "msedge" });
  try {
    const assets = await browser.newPage({ viewport: { width: 600, height: 168 }, deviceScaleFactor: 2 });
    for (const off of ["", "-off"]) {
      for (const kind of ["beam", "dot"]) {
        await assets.setContent(`<style>html,body{margin:0;background:transparent}${fx}
          .crt-fx-beam{left:0;right:0}</style>
          <div class="crt-fx crt-fx${off}" style="left:20px;top:20px;width:560px;height:128px">
          <div class="crt-fx-${kind}" style="opacity:1"></div></div>`);
        const dot = kind === "dot";
        await assets.screenshot({ omitBackground: true, clip: dot
          ? { x: 236, y: 20, width: 128, height: 128 }
          : { x: 20, y: 52, width: 560, height: 64 },
          path: path.join(out, `${kind}${off}.png`) });
      }
    }
    await assets.close();

    // Pause the actual reference keyframes to sample the same phases every run.
    const page = await browser.newPage({
      viewport: { width: 1480, height: 1050 }, reducedMotion: "no-preference",
    });
    for (const [name, width, height, radius] of [["monitor", 560, 321, 18], ["wallpapers", 1260, 900, 20]]) {
      for (const mode of ["open", "close"]) {
        await page.setContent(`<style>body{margin:0;background:#245fc3;display:grid;place-items:center;height:100vh}
          #box{width:${width}px;height:${height}px;background:#040416;border-radius:${radius}px}
          ${fx}</style><div id="box"></div>`);
        await page.addScriptTag({ content: `(() => {${animations}
          tube${mode === "open" ? "Open" : "Close"}(document.querySelector("#box"));
          window.tracks = document.getAnimations();
          window.tracks.forEach(a => { a.pause(); a.currentTime = 0; });
        })();` });
        if (await page.evaluate(() => window.tracks?.length) !== (mode === "open" ? 8 : 5)) {
          throw new Error(`Could not capture the ${name} ${mode} tracks`);
        }
        const times = mode === "open" ? [60, 145, 235, 315, 350, 380, 425, 500, 590, 700]
          : [55, 125, 172, 215, 260, 315, 365, 400];
        for (const ms of times) {
          await page.evaluate(ms => window.tracks.forEach(a => { a.currentTime = ms; }), ms);
          await page.screenshot({ path: path.join(out, `${name}-${mode}-${ms}.png`) });
        }
      }
    }
  } finally {
    await browser.close();
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
