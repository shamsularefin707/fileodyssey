const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: '/usr/bin/google-chrome', args: ['--no-sandbox'] });
  const errs = [];
  const shot = async (name, w, h, scheme, q, fn, full=false) => {
    const p = await b.newPage({ viewport: { width: w, height: h }, colorScheme: scheme });
    p.on('pageerror', e => errs.push(name + ': ' + e.message));
    p.on('console', m => { if (m.type() === 'error' && !m.text().includes('favicon')) errs.push(name + ': ' + m.text()); });
    await p.goto('http://localhost:1420/index.html' + q);
    await p.waitForTimeout(900);
    if (fn) await fn(p);
    await p.screenshot({ path: `${process.env.OUT || 'shots'}/${name}.png`, fullPage: full });
    await p.close();
  };
  await shot('desktop-light-empty', 1280, 800, 'light', '');
  await shot('desktop-light-ripple', 1280, 800, 'light', '', async p => { await p.click('text=choose files'); await p.waitForTimeout(300); });
  await shot('desktop-light-full', 1280, 800, 'light', '?demo=1', null, true);
  await shot('desktop-focus', 1280, 800, 'light', '?demo=1', async p => { for (let i=0;i<3;i++) await p.keyboard.press('Tab'); });
  await shot('desktop-run', 1280, 800, 'light', '', async p => { await p.click('text=choose files'); await p.waitForTimeout(600); await p.click('text=Run all'); await p.waitForTimeout(800); await p.mouse.wheel(0, 600); await p.waitForTimeout(200); });
  await shot('mobile-dark-report', 390, 844, 'dark', '?demo=1&report=1&platform=android');
  console.log(errs.length ? errs.join('\n') : 'no errors');
  await b.close();
})();
