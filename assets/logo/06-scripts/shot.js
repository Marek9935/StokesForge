const { chromium } = require('playwright');
const fs = require('fs'); const path = require('path');
(async () => {
  const dir = process.argv[2];
  const names = ['stokesforge-logo.svg','stokesforge-logo-dark.svg','stokesforge-logo-mono.svg','stokesforge-icon.svg','stokesforge-engraved.svg'];
  const imgs = names.map(n => `<div style="background:${n.includes('dark')?'#15181a':'#fff'};padding:8px;display:inline-block"><img width=256 src="data:image/svg+xml;base64,${fs.readFileSync(path.join(dir,n)).toString('base64')}"></div>`).join('');
  const small = names.filter(n=>!n.includes('dark')).map(n => `<img width=32 src="data:image/svg+xml;base64,${fs.readFileSync(path.join(dir,n)).toString('base64')}"> <img width=64 src="data:image/svg+xml;base64,${fs.readFileSync(path.join(dir,n)).toString('base64')}">`).join(' ');
  const b = await chromium.launch(); const p = await b.newPage({viewport:{width:1400,height:400}});
  await p.setContent(`<body style="margin:0;background:#ddd">${imgs}<div style="background:#fff;padding:8px">${small}</div></body>`);
  await p.screenshot({path: process.argv[3], fullPage:true}); await b.close();
})();
