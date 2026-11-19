const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 1280, height: 900 } });
  await p.goto('http://localhost:1420/index.html?demo=1&tab=images'); await p.waitForTimeout(1200);
  const row = p.locator('.file-row', { hasText: 'river at dusk.jpg' });
  await row.locator('select').first().selectOption({ label: 'JPEG' });
  await row.getByRole('button', { name: 'Options' }).click();
  await p.focus('#opt-quality');
  for (let i = 0; i < 5; i++) { await p.keyboard.press('ArrowRight'); await p.waitForTimeout(80); }
  const v = await p.locator('#opt-quality').inputValue();
  const focused = await p.evaluate(() => document.activeElement && document.activeElement.id);
  console.log(JSON.stringify({ value: v, focused }));
  console.log(v === '95' && focused === 'opt-quality' ? 'PASS' : 'FAIL');
  await b.close();
})();
