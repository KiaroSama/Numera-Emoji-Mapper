import { test as base, expect } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { PNG } from 'pngjs';
import { ROOT, ASSET_DIR } from './source-fixture.js';

const fixture = name => readFileSync(new URL(`./fixtures/${name}`, import.meta.url));
export const STUB = fixture('browser-stub.js').toString('utf8');
export const DENY_STORAGE = fixture('deny-storage.js').toString('utf8');
export const PROBE = fixture('perf-probe.js').toString('utf8');
export const REFERENCE = fixture('bulk-reference.js').toString('utf8');
export const sleep = ms => new Promise(done => setTimeout(done, ms));
export function synth(n, { logo = false, fmt = 'static', packs, excluded = [] } = {}) {
  const out = logo ? [{ key: '__brand_logo__', fmt: 'static', isLogo: true,
    label: 'Brand logo (auto-added on publish)', emoji: '', included: true }] : [];
  for (let i = 0; i < n; i++) {
    const item = { key: `x:${String(i).padStart(30, '0')}`, fmt, label: `item ${i}`,
      copyId: null, emoji: '', included: !Array.from(excluded).includes(i) };
    if (packs) item.pack = packs[i];
    out.push(item);
  }
  return out;
}

class Harness {
  constructor(browser, catalogSize) { this.browser = browser; this.catalogSize = catalogSize; }
  async start() {
    const name = process.platform === 'win32' ? 'numera-emoji.exe' : 'numera-emoji';
    const binary = process.env.NUMERA_BROWSER_BINARY || join(ROOT, 'runtime/target/release', name);
    if (!existsSync(binary)) throw new Error(`Build the standalone Rust panel before browser acceptance: cargo build --release --manifest-path runtime/Cargo.toml (missing ${binary})`);
    const parent = join(ROOT, 'logs/tmp/browser/fixtures');
    mkdirSync(parent, { recursive: true });
    this.root = mkdtempSync(join(parent, 'server-'));
    // The executable locates project assets beside itself. Isolate its logs/config/catalog too.
    mkdirSync(join(this.root, 'bin'));
    mkdirSync(join(this.root, 'assets'));
    for (const asset of ['panel.html', 'panel-grid.js', 'panel-motion.js', 'panel-save.js',
      'panel-drag.js', 'panel-actions.js', 'panel-holding.js', 'panel-draft.js', 'logo-128.png']) {
      copyFileSync(join(ASSET_DIR, asset), join(this.root, 'assets', asset));
    }
    const executable = join(this.root, 'bin', name);
    copyFileSync(binary, executable);
    this.data = join(this.root, 'collection');
    mkdirSync(join(this.data, 'media/static'), { recursive: true });
    this.database = join(this.data, 'catalog.db');
    const db = new DatabaseSync(this.database);
    try {
      db.exec(`CREATE TABLE items(content_key TEXT PRIMARY KEY,format TEXT NOT NULL,file_path TEXT NOT NULL,
        emojis TEXT NOT NULL DEFAULT '[]',keywords TEXT NOT NULL DEFAULT '[]',sources TEXT NOT NULL DEFAULT '[]',
        phash INTEGER,custom_emoji_id TEXT,uploaded INTEGER NOT NULL DEFAULT 0,included INTEGER NOT NULL DEFAULT 1,
        created_utc TEXT NOT NULL,position INTEGER);`);
      const insert = db.prepare('INSERT INTO items(content_key,format,file_path,keywords,created_utc,position) VALUES(?,?,?,?,?,?)');
      for (let i = 0; i < this.catalogSize; i++) {
        const png = new PNG({ width: 40, height: 40 });
        for (let pixel = 0; pixel < png.data.length; pixel += 4) png.data.set([9 * i, 20, 30, 255], pixel);
        const bytes = PNG.sync.write(png);
        writeFileSync(join(this.data, `media/static/i${i}.png`), bytes);
        if (!i) this.png = bytes;
        insert.run(`s:item${String(i).padStart(30, '0')}`, 'static', `./media/static/i${i}.png`, JSON.stringify([`item${i}`]), 'browser-fixture', i);
      }
    } finally { db.close(); }
    const reservation = createServer();
    await new Promise((done, fail) => reservation.once('error', fail).listen(0, '127.0.0.1', done));
    const port = reservation.address().port;
    await new Promise(done => reservation.close(done));
    this.url = `http://127.0.0.1:${port}/`;
    const env = Object.fromEntries(['PATH', 'SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'TMPDIR', 'LANG']
      .filter(key => process.env[key]).map(key => [key, process.env[key]]));
    env.NUMERA_EMOJI_MAPPER_NO_DOTENV = '1';
    this.output = '';
    this.process = spawn(executable, ['panel', '--data-dir', this.data, '--port', String(port), '--no-open', '--bot-username', 'YourEmojiBot'],
      { cwd: this.root, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    this.process.stdout.on('data', data => { this.output += data; });
    this.process.stderr.on('data', data => { this.output += data; });
    this.process.on('error', error => { this.startError = error; });
    for (const deadline = Date.now() + 10_000; Date.now() < deadline;) {
      if (this.startError || this.process.exitCode !== null) throw new Error(`Rust panel startup failed: ${this.startError || this.output}`);
      try { if ((await fetch(this.url + 'api/ping', { signal: AbortSignal.timeout(250) })).status === 200) return; } catch {}
      await sleep(10);
    }
    throw new Error(`Rust panel readiness exceeded 10s: ${this.output}`);
  }
  async stop() {
    if (this.process?.pid && this.process.exitCode === null && this.process.signalCode === null) {
      const closed = once(this.process, 'close', { signal: AbortSignal.timeout(10_000) });
      this.process.kill('SIGKILL');
      await closed;
    }
    if (this.root) rmSync(this.root, { recursive: true, force: true });
  }
  dbOrder() {
    const db = new DatabaseSync(this.database, { readOnly: true, timeout: 5000 });
    try { return db.prepare('SELECT content_key FROM items ORDER BY position,rowid').all().map(row => row.content_key); }
    finally { db.close(); }
  }
  excludedInDb() {
    const db = new DatabaseSync(this.database, { readOnly: true, timeout: 5000 });
    try { return new Set(db.prepare('SELECT content_key FROM items WHERE included=0').all().map(row => row.content_key)); }
    finally { db.close(); }
  }
  packPlan() { return JSON.parse(readFileSync(join(this.data, 'pack_plan.json'), 'utf8')); }
  async withCatalogLabels(labels, run) {
    const db = new DatabaseSync(this.database, { timeout: 5000 });
    let original;
    try {
      original = db.prepare('SELECT content_key,keywords FROM items ORDER BY position,rowid').all();
      if (labels.length > original.length || labels.some(label => typeof label !== 'string')) {
        throw new Error('Synthetic labels must name existing fixture rows');
      }
      const update = db.prepare('UPDATE items SET keywords=? WHERE content_key=?');
      for (const [i, label] of labels.entries()) update.run(JSON.stringify([label]), original[i].content_key);
    } finally { db.close(); }
    try { return await run(); }
    finally {
      const restore = new DatabaseSync(this.database, { timeout: 5000 });
      try {
        const update = restore.prepare('UPDATE items SET keywords=? WHERE content_key=?');
        for (const row of original) update.run(row.keywords, row.content_key);
      } finally { restore.close(); }
    }
  }
  reset() {
    const db = new DatabaseSync(this.database, { timeout: 5000 });
    try {
      db.exec('UPDATE items SET included=1');
      const keys = db.prepare('SELECT content_key FROM items ORDER BY content_key').all();
      const position = db.prepare('UPDATE items SET position=? WHERE content_key=?');
      for (const [i, row] of keys.entries()) position.run(i, row.content_key);
    } finally { db.close(); }
    rmSync(join(this.data, 'pack_plan.json'), { force: true });
  }
  async open(items, { init, clock = false, media = 'png', ...options } = {}, owner) {
    const context = await this.browser.newContext({ viewport: { width: 1200, height: 900 }, ...options });
    owner.contexts.push(context);
    const page = await context.newPage();
    page.on('pageerror', error => owner.errors.push(String(error)));
    if (clock) await page.clock.install();
    await page.addInitScript(STUB);
    if (init) await page.addInitScript(init);
    await page.route('**/*', async route => {
      if (new URL(route.request().url()).origin !== new URL(this.url).origin) return route.abort('blockedbyclient');
      return route.continue();
    });
    if (items !== undefined) await page.route(this.url, async route => {
      const served = await route.fetch();
      const payload = JSON.stringify(items).replaceAll('<', '\\u003c').replaceAll('>', '\\u003e').replaceAll('&', '\\u0026');
      const html = (await served.text()).replace(/(<script id="items-data" type="application\/json">).*?(<\/script>)/s,
        (_, start, end) => start + payload + end);
      await route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: html });
    });
    for (const pattern of ['**/img/**', '**/preview/**']) await page.route(pattern,
      route => route.fulfill({ status: 200, contentType: 'image/png', body: this.png }));
    if (media === 'animated') await page.route('**/preview/**', route => route.fulfill({ status: 200,
      contentType: 'image/webp', body: fixture(route.request().url().includes('still=1') ? 'still.webp' : 'moving.webp') }));
    await page.goto(this.url, { waitUntil: 'load', timeout: 30_000 });
    return page;
  }
}

export const test = base.extend({
  catalogSize: [8, { scope: 'worker', option: true }],
  harness: [async ({ browser, catalogSize }, use) => {
    const h = new Harness(browser, catalogSize);
    try { await h.start(); await use(h); } finally { await h.stop(); }
  }, { scope: 'worker' }],
  panel: async ({ harness }, use) => {
    const owner = { contexts: [], errors: [] };
    const panel = { open: (items, options) => harness.open(items, options, owner),
      dbOrder: () => harness.dbOrder(), excludedInDb: () => harness.excludedInDb(), reset: () => harness.reset(),
      packPlan: () => harness.packPlan(), withCatalogLabels: (labels, run) => harness.withCatalogLabels(labels, run), errors: owner.errors };
    try { await use(panel); } finally {
      for (const context of owner.contexts.reverse()) await context.close();
      expect(owner.errors, 'the page threw during the scenario').toEqual([]);
    }
  },
});
export { expect, Harness };
