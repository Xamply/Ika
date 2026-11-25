
/* Capture the authored controls and artwork. Development-only; the launcher renders natively. */
const fs = require('node:fs');
const path = require('node:path');
const legacyMissing = () => {
  throw new Error("Pass a checkout of the legacy-webview2 branch: node <script> <path> (or set IKA_LEGACY_DIR)");
};
const ref = path.resolve(process.argv[2] || process.env.IKA_LEGACY_DIR || legacyMissing());
const { chromium } = require(require.resolve('playwright', {paths:[ref]}));
const html=fs.readFileSync(path.join(ref,'index.html'),'utf8');
const css=fs.readFileSync(path.join(ref,'src/style.css'),'utf8').replace(/^@import.*$/gm,'');
const out=path.resolve('src/profile/art');
const proof=path.resolve('scratch/profile-reference');
fs.mkdirSync(out,{recursive:true});fs.mkdirSync(proof,{recursive:true});
const get=(file)=>fs.readFileSync(path.join(ref,'public',file));
const data=(file, mime)=>'data:'+mime+';base64,'+get(file).toString('base64');
const htmlCss=css.replaceAll('/cyber_doodle_pattern.svg',data('cyber_doodle_pattern.svg','image/svg+xml'));
const vars=[...new Set([...htmlCss.concat(html).matchAll(/--ik-([0-9a-f]{6})\b/gi)].map(m=>m[1]))]
  .map(hex=>'--ik-'+hex+':'+hex.match(/../g).map(n=>parseInt(n,16)).join(',')+';').join('');
(async()=>{
  const browser=await chromium.launch();
  try {
    const page=await browser.newPage({viewport:{width:1536,height:1000}});
    await page.setContent('<style>'+htmlCss+':root{'+vars+'}html,body{margin:0;background:#1b2640} *,*::before,*::after{animation:none!important;transition:none!important}</style>'+html.slice(html.indexOf('<svg id="hud-hairline-defs"'),html.indexOf('<!-- End HUD Hairline Defs')>0? html.indexOf('<!-- End HUD Hairline Defs'):html.indexOf('</svg>',html.indexOf('<svg id="hud-hairline-defs"'))+6)+html.slice(html.indexOf('<div id="modal-friends"'),html.indexOf('<!-- Acerca de Ika Launcher: in memory')));
    // The source is parsed without its main module. Populate only demo data.
    await page.evaluate(({avatar,cover})=>{
      for(const s of document.querySelectorAll('script, link')) s.remove();
      document.getElementById('profile-avatar-img').src=avatar;
      const h=document.querySelector('.profile-settings-header');
      h.style.setProperty('background-image','url("'+cover+'")','important');
      h.style.setProperty('background-size','cover','important');
      h.style.setProperty('background-position','center','important');
      document.querySelector('#friends-count').textContent='(2)';
      document.querySelector('#friends-empty').classList.add('hidden');
      document.querySelector('#friends-list').innerHTML=['AmigoUno','AmigoDos'].map(name=>
        '<li class="friends-row"><img class="friends-head" src="'+avatar+'"><div class="friends-meta"><span class="friends-name">'+name+'</span></div></li>').join('');
    },{avatar:data('steve-skin.png','image/png'),cover:data('profile-cyber-bg.jpg','image/jpeg')});
    for (const [name,id,box] of [['preferences','#modal-launcher-settings','.modal-profile-settings'],['friends','#modal-friends','.friends-modal-box']]) {
      await page.locator(id).evaluate(root=>root.classList.remove('hidden'));
      await page.screenshot({path:path.join(proof,name+'.png')});
      const selectors=name==='preferences'?['.profile-settings-header','.profile-settings-subhero','.profile-settings-search-wrap','.profile-settings-body','.settings-nav-row','.profile-avatar-wrap','.profile-badge-brand']
        :['.instance-creator-header','.friends-split','.friends-col-list','.friends-col-add','.friends-add-card','.wallpapers-header-pill','.friends-add-avatar'];
      const layout=await page.locator(box).evaluate((root,selectors)=>{
        const rect=r=>[r.x,r.y,r.width,r.height];
        const r=root.getBoundingClientRect();
        return {box:rect(r),elements:Object.fromEntries(selectors.map(s=>{
          const q=root.querySelector(s).getBoundingClientRect();return [s,[q.x-r.x,q.y-r.y,q.width,q.height]];
        }))};
      },selectors);
      console.log(name,JSON.stringify(layout));
      await page.locator(id).evaluate(root=>root.classList.add('hidden'));
    }
    fs.copyFileSync(path.join(ref,'public/profile-cyber-bg.jpg'),path.join(out,'default-cover.jpg'));
    fs.copyFileSync(path.join(ref,'public/laika/laika-1.webp'),path.join(out,'laika.webp'));
    const icons=await page.evaluate(()=>{
      const nodes=[...document.querySelectorAll('.settings-nav-icon-badge svg')];
      nodes.push(document.querySelector('.profile-search-input-box svg'), document.querySelector('.profile-cover-edit-btn svg'),
        document.querySelector('.friends-add-placeholder'),document.querySelector('.friends-empty-state svg'));
      return nodes.map(el=>el.outerHTML.replace(/class="[^"]*"/g,''));
    });
    await page.setViewportSize({width:72*(icons.length+1),height:72});
    const trash=html.match(/id="user-skin-btn-delete"[^>]*>\s*(<svg[\s\S]*?<\/svg>)/)?.[1];
    if (!trash) throw new Error('Missing skin delete icon');
    icons.push(trash);
    await page.setContent('<style>html,body{margin:0;background:transparent}body{display:flex;color:white}svg{display:block;width:60px;height:60px;flex:0 0 60px;margin:6px}</style>'+icons.join(''));
    await page.screenshot({omitBackground:true,scale:'css',path:path.join(out,'icons.png')});
    await page.setViewportSize({width:220,height:220});
    await page.setContent('<style>html,body{margin:0;background:transparent}img{display:block;width:220px;height:220px}</style><img src="'+data('cyber_doodle_pattern.svg','image/svg+xml')+'">');
    await page.locator('img').evaluate(el=>el.decode());
    await page.screenshot({omitBackground:true,path:path.join(out,'doodle.png')});
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});

