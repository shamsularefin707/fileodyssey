// Scan tab in the preview (Android): QR → Wi-Fi sheet, URL shown before
// opening, document review (reorder, remove) → saved → history, delete.
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 412, height: 915 }, deviceScaleFactor: 2 });
  const out = {};
  const base = 'http://localhost:1420/index.html?platform=android';
  await p.goto(base); await p.waitForTimeout(1200);
  await p.getByRole('button', { name: 'Scan', exact: true }).click(); await p.waitForTimeout(300);
  out.empty = await p.locator('.history .muted').first().innerText();
  // QR: camera overlay appears and the page goes see-through, then the sheet.
  await p.getByRole('button', { name: /Scan a QR code/ }).click();
  await p.waitForTimeout(200);
  out.cameraClass = await p.evaluate(() => document.documentElement.classList.contains('camera-on'));
  out.overlay = await p.locator('.camera-overlay').isVisible();
  await p.screenshot({ path: '/tmp/scan-camera.png' });
  await p.waitForTimeout(900);
  out.cameraOff = await p.evaluate(() => !document.documentElement.classList.contains('camera-on'));
  out.sheet = (await p.locator('.sheet').innerText()).replace(/\s+/g, ' ');
  await p.screenshot({ path: '/tmp/scan-wifi.png' });
  await p.getByRole('button', { name: 'Close' }).click();
  // URL: shown, opened only on tap.
  await p.goto(base + '&qr=url'); await p.waitForTimeout(1200);
  await p.getByRole('button', { name: 'Scan', exact: true }).click();
  await p.getByRole('button', { name: /Scan a QR code/ }).click(); await p.waitForTimeout(1200);
  out.urlBefore = await p.evaluate(() => window.__fo_last_url || null);
  out.urlShown = await p.locator('.sheet .url').innerText();
  await p.locator('.sheet').getByRole('button', { name: 'Open link' }).click(); await p.waitForTimeout(200);
  out.urlAfter = await p.evaluate(() => window.__fo_last_url);
  await p.getByRole('button', { name: 'Close' }).click();
  // Document: review, move page 3 up, remove page 1, save PDF + images.
  await p.getByRole('button', { name: /Scan a document/ }).click(); await p.waitForTimeout(500);
  out.pages = await p.locator('.page').count();
  await p.screenshot({ path: '/tmp/scan-review.png' });
  await p.locator('.page').nth(2).getByRole('button', { name: 'Move up' }).click();
  await p.locator('.page').nth(0).getByRole('button', { name: 'Remove' }).click();
  await p.getByLabel('Images (JPEG)').check();
  await p.getByRole('button', { name: 'Save', exact: true }).click(); await p.waitForTimeout(400);
  out.save = JSON.parse(await p.evaluate(() => window.__fo_last_save));
  out.saved = (await p.locator('.saved').innerText()).replace(/\s+/g, ' ');
  await p.getByRole('button', { name: 'Close' }).click(); await p.waitForTimeout(300);
  out.history = await p.locator('.history-item').count();
  await p.screenshot({ path: '/tmp/scan-home.png' });
  await p.locator('.history-item').first().getByRole('button', { name: 'Delete' }).click(); await p.waitForTimeout(300);
  out.afterDelete = await p.locator('.history-item').count();
  // No Play services: the document action is off and says why.
  await p.goto(base + '&noplay=1'); await p.waitForTimeout(1200);
  await p.getByRole('button', { name: 'Scan', exact: true }).click(); await p.waitForTimeout(300);
  out.noPlayDisabled = await p.getByRole('button', { name: /Scan a document/ }).isDisabled();
  out.noPlayText = await p.locator('.scan > p.muted, .scan p.muted').first().innerText();
  console.log(JSON.stringify(out, null, 1));
  const ok = out.cameraClass && out.overlay && out.cameraOff
    && out.sheet.includes('River House') && out.sheet.includes('heron-2026') && out.sheet.includes('Join network')
    && out.urlBefore === null && out.urlShown === 'https://example.org/survey' && out.urlAfter === 'https://example.org/survey'
    && out.pages === 3 && JSON.stringify(out.save.order) === '[2,1]' && out.save.pdf && out.save.jpegs
    && out.saved.includes('.pdf') && out.history === 2 && out.afterDelete === 1
    && out.noPlayDisabled && out.noPlayText.includes('Google Play services');
  console.log(ok ? 'PASS' : 'FAIL');
  await b.close();
})();
