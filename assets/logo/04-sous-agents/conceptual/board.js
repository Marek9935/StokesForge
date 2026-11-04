// Render board.png: every mark at 256 px with number + label, then 64 and 32 px rows.
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const dir = __dirname;
const marks = JSON.parse(fs.readFileSync(path.join(dir, 'marks.json'), 'utf8'));

const cell = (m, size) =>
  `<img src="data:image/svg+xml;base64,${Buffer.from(fs.readFileSync(path.join(dir, m.file))).toString('base64')}" width="${size}" height="${size}">`;

const html = `<!doctype html><html><head><meta charset="utf-8"><style>
body{margin:0;padding:28px;background:#dcd9d2;font-family:sans-serif;color:#22262a}
.row{display:flex;gap:28px;margin-bottom:22px;align-items:flex-end}
.big{display:flex;flex-direction:column;align-items:center;width:256px}
.big span{margin-top:8px;font-size:15px}
.small{display:flex;gap:28px}
.pair{width:256px;display:flex;gap:18px;align-items:center;justify-content:center}
.dark{background:#15181a;padding:10px 0;border-radius:8px}
</style></head><body>
<div class="row">${marks.map((m, i) => `<div class="big">${cell(m, 256)}<span><b>${i + 1}</b> &nbsp;${m.label}</span></div>`).join('')}</div>
<div class="row">${marks.map(m => `<div class="pair">${cell(m, 64)}${cell(m, 32)}</div>`).join('')}</div>
<div class="row dark">${marks.map(m => `<div class="pair">${cell(m, 64)}${cell(m, 32)}</div>`).join('')}</div>
</body></html>`;

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  await page.setContent(html);
  const w = await page.evaluate(() => document.body.scrollWidth);
  const h = await page.evaluate(() => document.body.scrollHeight);
  await page.setViewportSize({ width: w, height: h });
  await page.screenshot({ path: path.join(dir, process.argv[2] || 'board.png'), fullPage: true });
  await browser.close();
})();
