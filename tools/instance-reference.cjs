const fs=require('node:fs'),path=require('node:path');
const legacyMissing = () => {
  throw new Error("Pass a checkout of the legacy-webview2 branch: node <script> <path> (or set IKA_LEGACY_DIR)");
};
const ref=path.resolve(process.argv[2] || process.env.IKA_LEGACY_DIR || legacyMissing());
const {chromium}=require(require.resolve('playwright',{paths:[ref]}));
const html=fs.readFileSync(path.join(ref,'index.html'),'utf8');
const css=fs.readFileSync(path.join(ref,'src/style.css'),'utf8').replace(/^@import.*$/gm,'');
const vars=[...new Set([...html.concat(css).matchAll(/--ik-([0-9a-f]{6})([-\w]*)\b/gi)].map(m=>m[1]+m[2]))].map(k=>'--ik-'+k+':'+k.slice(0,6).match(/../g).map(x=>parseInt(x,16)).join(',')+';').join('');
const file=path.join(ref,'public','instance_covers/thumbs/shader_complementary_reimagined_1.jpg');
const cover='data:image/jpeg;base64,'+fs.readFileSync(file).toString('base64');
(async()=>{
const browser=await chromium.launch();
try {
const page=await browser.newPage({viewport:{width:1536,height:1000}});
await page.route('**/*',r=>r.abort());
const defs=html.slice(html.indexOf('<svg id="hud-hairline-defs"'),html.indexOf('</svg>',html.indexOf('<svg id="hud-hairline-defs"'))+6);
const dialogs=html.slice(html.indexOf('<div id="modal-create-instance"'),html.indexOf('<!-- Modal: Skins compartidas')).replace(/ onerror="[^"]*"/g,'');
await page.setContent('<style>'+css+':root{'+vars+'}body{background:#303046!important}*,*::before,*::after{animation:none!important;transition:none!important}</style>'+defs+dialogs);
await page.evaluate(cover=>{ for(const id of ['create-inst-cover-img','share-cover']){const el=document.getElementById(id);el.removeAttribute('onerror');el.src=cover;} document.getElementById('share-inst-name').textContent='Minecraft 1.21.4';document.getElementById('share-inst-meta').textContent='Minecraft 1.21.4 · Fabric · 12 mods'; },cover);
fs.mkdirSync('scratch/instances-reference',{recursive:true});
for(const [name,root,target] of [['create','#modal-create-instance','.modal-create-inst-modern'],['edit','#modal-create-instance','.modal-create-inst-modern'],['export','#modal-share','.share-modal-box']]){
await page.locator(root).evaluate((el,name)=>{el.classList.remove('hidden');if(name==='edit'){
  el.classList.add('is-edit-mode');
  el.querySelector('.create-inst-title').textContent='Editar Instancia';el.querySelector('#create-inst-edit-name').textContent='Mi instancia';
  el.querySelector('#section-search-modpack').classList.add('hidden');el.querySelector('#create-inst-right-modpack').classList.add('hidden');
  for(const s of ['#section-custom-mc','#create-inst-right-custom'])el.querySelector(s).classList.remove('hidden');
  el.querySelector('#btn-submit-create-inst span').textContent='Guardar cambios';
}},name);
await page.locator(target).screenshot({path:'scratch/instances-reference/'+name+'.png'});
console.log(name,await page.locator(target).evaluate(el=>{const p=el.getBoundingClientRect();return [p.width,p.height,Object.fromEntries([...el.querySelectorAll('.create-inst-header,.create-inst-title-pill,.create-inst-cover-card,.create-inst-row-icon-name,.create-inst-type-section,.create-inst-col-left,.create-inst-col-right,.create-inst-card,.create-inst-footer,.modpack-details-placeholder,.share-header-hero,.share-instance-card,.share-info-banner,.share-input-wrap,.share-actions')].filter(q=>q.getClientRects().length).map(q=>{const r=q.getBoundingClientRect();return [q.className,[r.x-p.x,r.y-p.y,r.width,r.height].map(Math.round)]}))]}));
await page.locator(root).evaluate(el=>el.classList.add('hidden'));
}
fs.copyFileSync(file,'src/home/instance-cover.jpg');
fs.copyFileSync(path.join(ref,'public/minecraft_block_textures/grass_block_side.png'),'src/home/instance-grass.png');
const icons=await page.evaluate(()=>['.share-pill-pc .share-pill-icon','.share-pill-export .share-pill-icon','.share-header-icon svg','.share-target-user-icon','.share-info-icon','.create-inst-title-pill > svg:not(.create-inst-title-border-svg)','#btn-type-modpack svg','#btn-type-custom svg','.modpack-empty-cube svg','.create-inst-version-card .create-inst-section-icon svg','.create-inst-loader-card .create-inst-section-icon svg','.create-inst-java-icon svg'].map(s=>document.querySelector(s)?.outerHTML??'<svg></svg>'));
await page.setViewportSize({width:72*icons.length,height:72});
await page.setContent('<style>html,body{margin:0;background:transparent}body{display:flex;color:white}body>svg{width:60px;height:60px;margin:6px;flex:0 0 60px}:root{'+vars+'}</style>'+defs.replace('<svg','<svg style="position:absolute;width:0;height:0;margin:0"')+icons.join(''));
await page.screenshot({omitBackground:true,path:'src/home/instance-icons.png'});
}finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
