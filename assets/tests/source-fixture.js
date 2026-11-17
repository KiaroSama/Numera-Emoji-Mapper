import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const ASSET_DIR = resolve(ROOT, 'assets');
export const PAGE = readFileSync(resolve(ASSET_DIR, 'panel.html'), 'utf8');
export const SCRIPT = ['grid', 'motion', 'save', 'drag', 'actions', 'holding', 'draft']
  .map(name => readFileSync(resolve(ASSET_DIR, `panel-${name}.js`), 'utf8')).join('\n');
// Read the production handler's injected cap rather than assert a test-owned constant.
const handlerPath = ['runtime/src/panel_http.rs', 'native/src/panel_http.rs']
  .map(path => resolve(ROOT, path)).find(path => existsSync(path));
const handler = readFileSync(handlerPath, 'utf8');
const capacity = handler.match(/\("__PER_SET__",\s*"(\d+)"\.into\(\)\)/);
if (!capacity) throw new Error('The production panel handler must inject the per-set cap');
export const PER_SET = Number(capacity[1]);
export function index(source, needle, start = 0) {
  const at = source.indexOf(needle, start);
  if (at < 0) throw new Error(`Required source marker missing: ${needle}`);
  return at;
}
export function block(source, start, end) {
  const at = index(source, start);
  return source.slice(at, index(source, end, at));
}
