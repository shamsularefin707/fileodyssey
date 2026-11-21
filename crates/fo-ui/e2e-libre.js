// LibreOffice targets: disabled with a reason when it isn't installed,
// a "Get LibreOffice" link for files only it can convert, and normal
// targets when it is (`?libreoffice=1`).
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 1280, height: 900 } });
  const out = {};
  await p.goto('http://localhost:1420/index.html?demo=1&tab=convert'); await p.waitForTimeout(1200);
  const doc = p.locator('.file-row', { hasText: 'committee minutes 2004.doc' });
  out.docMessage = (await doc.locator('.needs').innerText()).trim();
  await doc.getByRole('button', { name: 'Get LibreOffice' }).click();
  await p.waitForTimeout(200);
  out.link = await p.evaluate(() => window.__fo_last_link);
  const docx = p.locator('.file-row', { hasText: '.docx' });
  const pdfOpt = docx.locator('option', { hasText: 'PDF — needs LibreOffice' });
  out.docxPdfDisabled = await pdfOpt.evaluate(o => o.disabled);
  out.docxSelected = await docx.locator('select').evaluate(s => s.options[s.selectedIndex].text);
  await p.screenshot({ path: '/tmp/libre-missing.png' });
  await p.goto('http://localhost:1420/index.html?demo=1&tab=convert&libreoffice=1'); await p.waitForTimeout(1200);
  const doc2 = p.locator('.file-row', { hasText: 'committee minutes 2004.doc' });
  out.docTargets = await doc2.locator('option').allInnerTexts();
  await p.screenshot({ path: '/tmp/libre-present.png' });
  console.log(JSON.stringify(out));
  const ok = out.docMessage.startsWith('Install LibreOffice to enable') && out.link === 'office.convert'
    && out.docxPdfDisabled === true && !out.docxSelected.includes('needs')
    && JSON.stringify(out.docTargets) === JSON.stringify(['DOCX', 'ODT', 'RTF', 'PDF']);
  console.log(ok ? 'PASS' : 'FAIL');
  await b.close();
})();
