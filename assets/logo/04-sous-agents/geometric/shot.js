const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({deviceScaleFactor:1});
  await p.goto('file://' + __dirname + '/board.html');
  await p.screenshot({ path: __dirname + '/board.png', fullPage: true }); await b.close();
})();
