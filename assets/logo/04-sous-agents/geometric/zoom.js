const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage();
  const f=['m1-press','m2-anvil-of-light','m3-seal','m4-columns','m5-tile'];
  await p.goto('file://'+__dirname+'/zoom.html');
  await p.waitForTimeout(300);
  await p.screenshot({ path: __dirname + '/zoom.png', fullPage: true }); await b.close();
})();
