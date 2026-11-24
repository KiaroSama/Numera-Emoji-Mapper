// Exercise the actual roster script with real browser observers and media.
import { readFileSync } from 'node:fs';
import { expect, test } from '../browser-fixture.js';

const script = readFileSync(new URL('../../pack-gallery.js', import.meta.url), 'utf8');
const css = readFileSync(new URL('../../pack-gallery.css', import.meta.url), 'utf8');
const data = (path, type) => `data:${type};base64,${readFileSync(new URL(path, import.meta.url)).toString('base64')}`;
const still = data('./still.webp', 'image/webp');
const moving = data('./moving.webp', 'image/webp');
const video = data('../../../tests/fixtures/native/video-codec/alpha.webm', 'video/webm');
const LEVELS = new Set(['INFO', 'WARNING', 'ERROR', 'DEBUG']);
function log(level, message) {
  if (!LEVELS.has(level)) throw new Error('Unknown gallery regression log level');
  console.log(`${new Date().toISOString()} [${level}] ${message}`);
}

async function openGallery(panel, options = {}) {
  const page = await panel.open([], options);
  const url = new URL('/gallery-motion-fixture', page.url()).href;
  const image = `<img alt="Fixture animation" src="${still}" data-still="${still}" data-anim="${moving}">`;
  const clip = `<video muted loop playsinline data-play="1" src="${video}"></video>`;
  const cards = Array.from({ length: 120 }, (_, i) =>
    `<div class="card fmt-${i === 1 ? 'video' : 'animated'}"><div class="thumb">${i === 1 ? clip : image}</div></div>`).join('');
  const html = `<!doctype html><meta charset="utf-8"><style>${css}</style>
    <header><button id="top">Top</button><button id="bot">Bottom</button>
    <button id="bg">Backdrop</button><button id="anim"><span id="animLabel"></span></button></header>
    <main class="grid">${cards}</main><script>${script}</script>`;
  await page.route(url, route => route.fulfill({ status: 200, contentType: 'text/html', body: html }));
  await page.goto(url);
  await page.evaluate(() => {
    window.__galleryHidden = false;
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => window.__galleryHidden });
    window.__galleryMoving = () => [...document.querySelectorAll('img[data-anim]')]
      .filter(image => image.getAttribute('src') === image.dataset.anim).length;
    window.__galleryHide = hidden => {
      window.__galleryHidden = hidden;
      document.dispatchEvent(new Event('visibilitychange'));
    };
    window.__galleryScroll = () => {
      window.__galleryScrolling = true;
      const step = () => {
        if (!window.__galleryScrolling) return;
        scrollBy(0, scrollY > 20 ? -1 : 1);
        requestAnimationFrame(step);
      };
      step();
    };
  });
  return page;
}

// Re-observation requests a fresh native delivery. The second real observer
// acknowledges that delivery, so a passing assertion cannot precede the race.
async function deliverObserver(page) {
  await page.evaluate(() => {
    window.__galleryTick = 0;
    const probe = new IntersectionObserver(() => { window.__galleryTick++; probe.disconnect(); });
    for (const card of cards()) { io.unobserve(card); io.observe(card); probe.observe(card); }
  });
  await page.waitForFunction('window.__galleryTick > 0');
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

async function frozen(page) {
  expect(await page.evaluate('window.__galleryMoving()'), 'gallery images restarted during a freeze').toBe(0);
  expect(await page.locator('video').evaluate(video => video.paused), 'gallery video restarted during a freeze').toBe(true);
}

export async function runGalleryFreezeScenarios(panel) {
  const page = await openGallery(panel);
  await page.waitForFunction('window.__galleryMoving() > 0');
  await page.waitForFunction('!document.querySelector("video").paused');

  await test.step('Gallery observers cannot restart a hidden page', async () => {
    await page.evaluate('window.__galleryHide(true)');
    await frozen(page);
    await deliverObserver(page);
    await frozen(page);
    await page.evaluate('window.__galleryHide(false)');
    await page.waitForFunction('window.__galleryMoving() > 0');
  });

  await test.step('Gallery scrolling, switches and visibility share one freeze', async () => {
    await page.evaluate('window.__galleryScroll()');
    await page.waitForFunction('thaw !== null');
    await deliverObserver(page);
    await frozen(page);
    await page.locator('#anim').click();
    await page.locator('#anim').click();
    await deliverObserver(page);
    await frozen(page);
    await page.evaluate('window.__galleryHide(true); window.__galleryHide(false)');
    await deliverObserver(page);
    await frozen(page);
    await page.evaluate('window.__galleryHide(true); window.__galleryScrolling = false');
    await page.waitForFunction('thaw === null');
    await deliverObserver(page);
    await frozen(page);
    await page.evaluate('window.__galleryHide(false)');
    await page.waitForFunction('window.__galleryMoving() > 0');
    await page.waitForFunction('!document.querySelector("video").paused');
  });

  await test.step('Gallery master Off and reduced motion remain frozen', async () => {
    await page.locator('#anim').click();
    await deliverObserver(page);
    await frozen(page);
    const reduced = await openGallery(panel, { reducedMotion: 'reduce' });
    await deliverObserver(reduced);
    await frozen(reduced);
  });
  log('INFO', 'Gallery freeze checks passed for hidden, scroll, toggle, re-entry and reduced-motion states.');
}
