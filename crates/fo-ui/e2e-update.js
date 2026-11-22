// Updates: banner on start when allowed, Not now remembers the version,
// Update and restart (desktop), Download (Android), the switch, Check now
// offline (says so only when asked).
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 1280, height: 900 } });
  const out = {};
  const url = 'http://localhost:1420/index.html';
  await p.goto(url + '?update=1'); await p.waitForTimeout(1200);
  out.banner = await p.locator('.update-banner').innerText();
  await p.screenshot({ path: '/tmp/update-banner.png' });
  await p.getByRole('button', { name: 'Update and restart' }).click(); await p.waitForTimeout(200);
  out.installed = await p.evaluate(() => window.__fo_installed);
  await p.goto(url + '?update=1'); await p.waitForTimeout(1200);
  await p.getByRole('button', { name: 'Not now' }).click(); await p.waitForTimeout(200);
  out.afterNotNow = await p.locator('.update-banner').count();
  out.skipped = JSON.parse(await p.evaluate(() => window.__fo_settings)).skipped;
  // Android: Download opens the release page.
  await p.goto(url + '?update=1&platform=android'); await p.waitForTimeout(1200);
  await p.locator('.update-banner').getByRole('button', { name: 'Download' }).click(); await p.waitForTimeout(200);
  out.releasePage = await p.evaluate(() => window.__fo_release_page);
  // Offline at start: nothing shown; Check now says so.
  await p.goto(url + '?update=offline'); await p.waitForTimeout(1200);
  out.offlineBanner = await p.locator('.update-banner').count();
  await p.locator('.about summary').click();
  await p.getByRole('button', { name: 'Check now' }).click(); await p.waitForTimeout(300);
  out.offlineStatus = await p.locator('.about [role=status]').innerText();
  await p.getByLabel(/Check for a new version/).uncheck(); await p.waitForTimeout(200);
  out.switchOff = JSON.parse(await p.evaluate(() => window.__fo_settings)).check_updates;
  out.version = await p.locator('.about p').first().innerText();
  await p.screenshot({ path: '/tmp/update-about.png' });
  console.log(JSON.stringify(out, null, 1));
  const ok = out.banner.includes('FileOdyssey 1.0.1 is available') && out.installed === '1'
    && out.afterNotNow === 0 && out.skipped === '1.0.1' && out.releasePage === '1'
    && out.offlineBanner === 0 && out.offlineStatus.includes("Couldn't reach GitHub")
    && out.switchOff === false && out.version.includes('1.0.0');
  console.log(ok ? 'PASS' : 'FAIL');
  await b.close();
})();
