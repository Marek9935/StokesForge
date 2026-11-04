const { chromium } = require('playwright'); const fs=require('fs');
const html = fs.readFileSync('ds/project/components/Cover/preview.html','utf8');
const L=':root{--ground:#fff;--surface:#f3f1ec;--ink:#22262a;--ink-muted:#5f6b64;--forge-green:#3a9155;--ember:#e0651a;--radius-md:8px;--space-2:8px;--space-6:24px;--font-sans:system-ui,sans-serif}';
const D=':root{--ground:#15181a;--surface:#1f2326;--ink:#e6e9e7;--ink-muted:#a3ada7;--forge-green:#5cc07d;--ember:#f4894a;--radius-md:8px;--space-2:8px;--space-6:24px;--font-sans:system-ui,sans-serif}';
(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:960,height:288}});
for (const [n,v] of [['L',L],['D',D]]){await p.setContent(html.replace('<style>','<style>'+v));await p.screenshot({path:`cover${n}.png`});}
await b.close();})();
