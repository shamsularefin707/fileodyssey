// Combine bar must archive the files listed *now*, even when the count
// stays the same after removing one file and adding another.
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const b = await chromium.launch({ executablePath: process.env.CHROME || undefined, args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 1280, height: 900 } });
  await p.goto((process.env.URL || 'http://localhost:1420/index.html') + '?tab=compress');
  await p.waitForTimeout(900);
  await p.getByRole('button', { name: 'choose files' }).click(); await p.waitForTimeout(800);
  await p.locator('.file-row', { hasText: 'meeting notes.md' }).getByRole('button', { name: /Remove/ }).click();
  await p.waitForTimeout(200);
  await p.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File(['x'], 'extra.txt'));
    const pond = document.querySelector('.pond');
    pond.dispatchEvent(new DragEvent('drop', { dataTransfer: dt, bubbles: true, cancelable: true }));
  });
  await p.waitForTimeout(800);
  await p.getByRole('button', { name: 'Create archive' }).click(); await p.waitForTimeout(400);
  const req = JSON.parse(await p.evaluate(() => window.__fo_last_job || '{}'));
  const inputs = (req.inputs || []).map(s => s.split('/').pop());
  console.log(JSON.stringify(inputs));
  const ok = inputs.includes('extra.txt') && !inputs.includes('meeting notes.md');
  console.log(ok ? 'PASS' : 'FAIL');
  await b.close();
  process.exit(ok ? 0 : 1);
})();
