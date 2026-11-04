const { chromium } = require('playwright');
const path = require('path');
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1500, height: 480 } });
  await p.goto('file://' + path.join(__dirname, 'board.html'));
  await p.screenshot({ path: path.join(__dirname, 'board.png'), fullPage: true });
  await b.close();
})();
