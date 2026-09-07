# Explorer Frontend (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A SvelteKit static SPA at `frontend/` that renders every phase‑1 route of the explorer (home, blocks, block, txs, tx, box, address, rich list, rent, search, status) against the `/v1` API, with exact amounts, rent views, keyboard search, dark/light themes, tests, a bundle budget and a one-command deploy to Nuremberg.

**Architecture:** SvelteKit 2 + Svelte 5 (runes) compiled with `@sveltejs/adapter-static` to a client-only SPA (`fallback: 'index.html'`). Data is loaded in SvelteKit `+page.ts` `load` functions through a thin typed API client; SvelteKit's native `data-sveltekit-preload-data="hover"` gives prefetch-on-hover; infinite lists fetch further pages client-side. Styling is plain CSS on design tokens. Amounts are handled as `bigint` end to end.

**Tech Stack:** Node 22, npm, SvelteKit 2, Svelte 5, TypeScript, Vite, `@sveltejs/adapter-static`, Vitest, Playwright, MSW (mock API in e2e), rsync + Caddy for deploy.

**Spec:** `docs/superpowers/specs/2026-09-06-explorer-frontend-design.md`. **Amendment (this plan):** the spec's TanStack Query layer is replaced by SvelteKit `load` + native preload + a small client-side pager; it removes a dependency whose Svelte‑5 support is unsettled and gives the same caching/prefetch behaviour for our read-only pages. Everything else in the spec stands.

## Global Constraints

- Amounts (`value`, `fee`, `nano`, `due_nano`, `fees`, `reward`, token `amount`) are decimal strings from the API and MUST be converted with `BigInt`, never `Number` (spec §5).
- API base is `/v1` on the same origin; problem JSON errors `{type,title,status,detail}` (spec §5, §7).
- Cursor pagination: `PageDto<T> = { items: T[], next_cursor: string | null }`; lists request `limit ≤ 500` (spec §5).
- Home route bundle budget: ≤ 120 KB gzipped JavaScript, enforced by a script that fails the build (spec §8).
- Dark-first theme with light theme; default follows `prefers-color-scheme`; manual toggle persisted in `localStorage` (spec §2).
- Contrast ≥ 4.5:1, focus rings, keyboard reachability, `prefers-reduced-motion` respected (spec §4).
- Density: 13–14px body, 12px mono in tables, 36px rows (spec §4).
- No CSS framework, no webfont downloads (system font stacks) (spec §4).
- Every commit: `npm run check && npm run lint && npm test` green (in `frontend/`).
- Commit trailer: `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_01Jegbu3j98VqxmXFrcFCFdS`.

---

## File structure

```
frontend/
  package.json  svelte.config.js  vite.config.ts  tsconfig.json  playwright.config.ts
  static/favicon.svg
  src/app.html  src/app.d.ts
  src/lib/styles/tokens.css        # design tokens, both themes
  src/lib/styles/base.css          # reset, typography, density
  src/lib/api/types.ts             # DTO mirrors
  src/lib/api/client.ts            # apiGet<T>, ApiError
  src/lib/api/endpoints.ts         # typed functions per route
  src/lib/format/amount.ts         # formatErg / formatNano (BigInt)
  src/lib/format/time.ts           # relative/absolute time
  src/lib/format/hash.ts           # truncateMiddle
  src/lib/search/classify.ts       # classify(q)
  src/lib/theme/theme.svelte.ts    # theme store (runes)
  src/lib/pager/pager.svelte.ts    # createPager<T>(fetchPage) for infinite lists
  src/lib/components/{Hash,Amount,Age,Badge,Panel,Table,InfiniteList,Tabs,EmptyState,ErrorState,Skeleton,SearchBox,StatusBadge,RentBadge}.svelte
  src/routes/+layout.svelte  +layout.ts  +error.svelte
  src/routes/+page.svelte  +page.ts                       # home
  src/routes/status/+page.svelte +page.ts
  src/routes/blocks/+page.svelte +page.ts
  src/routes/blocks/[id]/+page.svelte +page.ts
  src/routes/txs/+page.svelte +page.ts
  src/routes/tx/[id]/+page.svelte +page.ts
  src/routes/box/[id]/+page.svelte +page.ts
  src/routes/address/[addr]/+page.svelte +page.ts
  src/routes/richlist/+page.svelte +page.ts
  src/routes/rent/+page.svelte +page.ts
  src/routes/search/+page.ts                              # resolves & redirects
  tests/unit/*.test.ts                                    # vitest
  tests/e2e/mock/{handlers.ts,fixtures.ts}                # MSW handlers built from ../../tests/fixtures/blocks
  tests/e2e/*.spec.ts                                     # playwright
  scripts/bundle-budget.mjs
scripts/deploy_frontend.sh                                # repo root
```

---

### Task 1: Scaffold, design tokens, shell layout, tooling

**Files:** create everything under `frontend/` listed above for config, styles, `app.html`, `+layout.svelte`, `+layout.ts`, `+error.svelte`, `theme.svelte.ts`, and `scripts/bundle-budget.mjs`.

**Interfaces produced:** `theme.svelte.ts` exports `theme` (`{ current: 'dark'|'light', toggle(): void, init(): void }`); CSS variables `--bg --bg-elev --bg-hover --fg --fg-muted --accent --ok --warn --danger --border --radius --radius-lg --space-1..8 --font-sans --font-mono`; layout slot renders pages inside `<main class="content">`.

- [ ] **Step 1: Scaffold**

```bash
cd /home/rkadias/coding/development/arkadianet/ergo-explorer
npx --yes sv@latest create frontend --template minimal --types ts --no-add-ons --install npm
cd frontend
npm i -D @sveltejs/adapter-static vitest @playwright/test msw eslint prettier prettier-plugin-svelte eslint-plugin-svelte typescript-eslint @vitest/coverage-v8
```
`svelte.config.js`:
```js
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
export default {
  preprocess: vitePreprocess(),
  kit: { adapter: adapter({ fallback: 'index.html', strict: false }), prerender: { entries: [] } }
};
```
`src/routes/+layout.ts`: `export const ssr = false; export const prerender = false;`
`vite.config.ts` adds `server: { proxy: { '/v1': 'http://127.0.0.1:18090' } }` and `test: { include: ['tests/unit/**/*.test.ts'], environment: 'node' }`.
`package.json` scripts: `dev`, `build`, `preview`, `check` (`svelte-kit sync && svelte-check --tsconfig ./tsconfig.json`), `lint` (`prettier --check . && eslint .`), `format`, `test` (`vitest run`), `test:e2e` (`playwright test`), `budget` (`node scripts/bundle-budget.mjs`), and `postbuild: npm run budget`.

- [ ] **Step 2: Tokens and base CSS**

`src/lib/styles/tokens.css`:
```css
:root { color-scheme: dark;
  --bg:#0e1116; --bg-elev:#151a21; --bg-hover:#1c232c; --fg:#e6e9ee; --fg-muted:#8b95a5;
  --accent:#4f8cff; --ok:#3ecf8e; --warn:#f0b429; --danger:#f0616d; --border:#232b36;
  --radius:6px; --radius-lg:10px;
  --space-1:4px; --space-2:8px; --space-3:12px; --space-4:16px; --space-5:20px; --space-6:24px; --space-8:32px;
  --font-sans: system-ui, -apple-system, "Segoe UI", Roboto, Ubuntu, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
:root[data-theme="light"] { color-scheme: light;
  --bg:#f7f8fa; --bg-elev:#ffffff; --bg-hover:#eef1f5; --fg:#151a21; --fg-muted:#5b6675;
  --accent:#2f6fe4; --ok:#188a5b; --warn:#a56f00; --danger:#c0343f; --border:#dde2e9; }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation: none !important; transition: none !important; } }
```
`base.css`: reset, `body{background:var(--bg);color:var(--fg);font:14px/1.45 var(--font-sans)}`, `.mono{font:12px var(--font-mono)}`, `:focus-visible{outline:2px solid var(--accent);outline-offset:2px}`, `a{color:var(--accent)}`, table row height 36px.

- [ ] **Step 3: Theme store**

`src/lib/theme/theme.svelte.ts`:
```ts
type Mode = 'dark' | 'light';
const KEY = 'xp-theme';
function systemMode(): Mode { return matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'; }
let current = $state<Mode>('dark');
function apply(m: Mode) { current = m; document.documentElement.dataset.theme = m; }
export const theme = {
  get current() { return current; },
  init() { let saved: string | null = null; try { saved = localStorage.getItem(KEY); } catch {} apply(saved === 'light' || saved === 'dark' ? saved : systemMode()); },
  toggle() { const m: Mode = current === 'dark' ? 'light' : 'dark'; apply(m); try { localStorage.setItem(KEY, m); } catch {} }
};
```
Inline script in `app.html` `<head>` sets `data-theme` before first paint using the same key/logic (avoids flash).

- [ ] **Step 4: Shell layout**

`+layout.svelte`: imports both CSS files; top bar with brand link, `<slot name=search>` placeholder (Task 10 fills with `SearchBox`), `StatusBadge` placeholder (Task 4), theme toggle button (`aria-label="Toggle theme"`); left rail `<nav aria-label="Primary">` with links Blocks `/blocks`, Transactions `/txs`, Rich list `/richlist`, Rent `/rent`, Status `/status`; `data-sveltekit-preload-data="hover"` on `<body>` via `app.html`; grid `grid-template-columns: 220px 1fr` at ≥1024px, bottom nav below; `<main class="content" style="max-width:1280px">`. `+error.svelte` shows status + message + a link home.

- [ ] **Step 5: Bundle budget script**

`scripts/bundle-budget.mjs`: after build, gzip every `build/_app/immutable/**/*.js` referenced by the home route's chunks — simplest correct approach: gzip ALL `entry/*.js` + `chunks/*.js` + `nodes/0.*.js` + `nodes/2.*.js` (layout + home) and sum; fail with exit 1 if > 120*1024; print the table. Uses `node:zlib` and `node:fs` only.

- [ ] **Step 6: Verify and commit**

Run: `npm run check && npm run lint && npm run build` → build succeeds, budget prints (tiny at this stage). `npm run dev` renders the shell at http://localhost:5173 with working theme toggle (manual check; note in report).
```bash
git add frontend && git commit -m "feat(frontend): SvelteKit static scaffold, design tokens, shell layout, theme, bundle budget"
```

---

### Task 2: API types, client, formatters, search classifier (unit-tested)

**Files:** `src/lib/api/types.ts`, `client.ts`, `endpoints.ts`, `src/lib/format/{amount,time,hash}.ts`, `src/lib/search/classify.ts`, tests `tests/unit/{amount,classify,hash,client}.test.ts`.

**Interfaces produced:**
```ts
// types.ts — mirrors crates/xp-api/src/dto.rs
export interface PageDto<T> { items: T[]; next_cursor: string | null }
export interface StatusDto { indexed: number | null; best: number; mode: 'bulk'|'tip'; source: string; halted: string | null; lag_blocks: number }
export interface BlockDto { id: string; height: number; parent_id: string; timestamp: number; difficulty: string; miner_pk: string; tx_count: number; size: number; fees: string; reward: string; version: number }
export interface TokenDto { id: string; amount: string }
export interface RentDto { maturity_height: number; due_nano: string; claimable_at_tip: boolean }
export interface BoxDto { id: string; tx_id: string; index: number; value: string; creation_height: number; ergo_tree: string | null; address: string | null; template_hash: string | null; tree_hash: string; tokens: TokenDto[]; registers: Record<string, unknown> | null; size: number; spent_by: string | null; spent_height: number | null; rent: RentDto }
export interface InputDto { id: string; box: BoxDto | null }
export interface TxDto { id: string; height: number; index: number; timestamp: number; size: number; fee: string; inputs: InputDto[]; data_inputs: string[]; outputs: BoxDto[] }
export interface BalanceDto { nano: string; tokens: TokenDto[] }
export interface AddressDto { address: string; tree_hash: string; balance: BalanceDto; box_count: number; first_seen: number; last_seen: number }
export interface AddressRentDto { items: BoxDto[]; truncated: boolean }
export interface RentItemDto { maturity_height: number; box: BoxDto }
export interface RichlistItemDto { address: string | null; tree_hash: string; nano: string }
export interface SearchDto { kind: 'block'|'tx'|'box'|'address'; id: string }
// client.ts
export class ApiError extends Error { constructor(public status: number, public title: string, public detail: string) { super(`${status} ${title}: ${detail}`); } }
export async function apiGet<T>(path: string, params?: Record<string, string | number | boolean | undefined>, fetchFn: typeof fetch = fetch): Promise<T>
// endpoints.ts — one function per route, e.g.
export const api = { status: (f?) => apiGet<StatusDto>('/status', undefined, f), blocks: (cursor?: string, limit = 50, f?) => …, block: (id, f?) => …, blockTxs: (id, f?) => …, txs: …, tx: …, box: …, address: …, addressTxs: …, addressBoxes: (addr, unspent: boolean, cursor?, limit?, f?) => …, addressRent: …, richlist: (cursor?, limit?, f?) => …, rentUpcoming: (blocks = 720, limit = 50, f?) => …, rentEligible: (cursor?, limit?, f?) => …, search: (q, f?) => … }
// amount.ts
export function formatErg(nano: string | bigint, opts?: { maxFrac?: number }): string   // "1,412,124.000000000" trimmed to significant, min 2 frac when non-integer
export function formatNano(nano: string | bigint): string                             // grouped integer
export const NANO = 1_000_000_000n;
// time.ts
export function relTime(msSinceEpoch: number, now = Date.now()): string  // "12 s ago", "3 min ago", "2 h ago", "5 d ago"
export function absTime(ms: number): string                              // ISO-like local "2026-09-05 17:25:44"
// hash.ts
export function truncateMiddle(s: string, head = 8, tail = 6): string  // "aa44ef6a…95a4d6"
// classify.ts
export type Classified = { kind: 'height'; value: number } | { kind: 'hex32'; value: string } | { kind: 'address'; value: string } | { kind: 'unknown'; value: string }
export function classify(q: string): Classified
```

- [ ] **Step 1: Failing unit tests**

`tests/unit/amount.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import { formatErg, formatNano } from '$lib/format/amount';
describe('formatErg', () => {
  it('handles zero and one nanoERG exactly', () => { expect(formatErg('0')).toBe('0'); expect(formatErg('1')).toBe('0.000000001'); });
  it('formats whole ERG with grouping', () => { expect(formatErg('12000000000')).toBe('12'); expect(formatErg('1412124000000000')).toBe('1,412,124'); });
  it('keeps exact fractions, trims trailing zeros', () => { expect(formatErg('1500000000')).toBe('1.5'); expect(formatErg('1000000001')).toBe('1.000000001'); });
  it('never loses precision above 2^53', () => { expect(formatErg('93409132500000000')).toBe('93,409,132.5'); });
  it('maxFrac rounds down (truncates) not up', () => { expect(formatErg('1999999999', { maxFrac: 2 })).toBe('1.99'); });
});
describe('formatNano', () => { it('groups', () => expect(formatNano('1250000')).toBe('1,250,000')); });
```
`tests/unit/classify.test.ts`:
```ts
import { classify } from '$lib/search/classify';
it('height', () => expect(classify(' 1866000 ')).toEqual({ kind: 'height', value: 1866000 }));
it('hex32 lowercased', () => expect(classify('AA44EF6A6C08D0B198D65B762ABB0181E6AD995654116FBEDAA1D3D3EB95A4D6')).toEqual({ kind: 'hex32', value: 'aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6' }));
it('address', () => expect(classify('9i5FJNkbtZH8kcS129ny71wLBK7cKhJ7ixLpLTZoirjXRFhfvs').kind).toBe('address'));
it('unknown', () => { expect(classify('hello').kind).toBe('unknown'); expect(classify('0x1234').kind).toBe('unknown'); expect(classify('').kind).toBe('unknown'); });
```
`tests/unit/hash.test.ts`: `truncateMiddle('aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6')` → `'aa44ef6a…95a4d6'`; short strings returned unchanged.
`tests/unit/client.test.ts`: stub `fetch` returning 404 problem JSON → `apiGet` rejects with `ApiError{status:404,title,detail}`; 200 → parsed body; query params serialised, `undefined` skipped.

- [ ] **Step 2: Run** `npm test` → fails (modules missing).

- [ ] **Step 3: Implement** all files per the Interfaces block. `formatErg`: `const n = BigInt(x); const whole = n / NANO; const frac = n % NANO;` group `whole` with a manual regex (`Intl` is fine for grouping but must be fed a string not a Number: use `whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')`); frac padded to 9, trimmed of trailing zeros, truncated to `maxFrac` if given. `classify`: trim; `/^\d{1,9}$/` → height; `/^[0-9a-fA-F]{64}$/` → hex32 lowercased; `/^[9382][1-9A-HJ-NP-Za-km-z]{30,}$/` → address; else unknown. `apiGet`: `AbortController` 10 s timeout; non-2xx → parse problem JSON (fallback title = statusText) → throw `ApiError`.

- [ ] **Step 4: Run** `npm test` → all pass. `npm run check && npm run lint` clean.

- [ ] **Step 5: Commit** `feat(frontend): typed API client, BigInt amount formatting, time/hash helpers, search classifier`

---

### Task 3: Core components

**Files:** `src/lib/components/{Hash,Amount,Age,Badge,Panel,Table,Tabs,EmptyState,ErrorState,Skeleton,RentBadge}.svelte`, `src/lib/pager/pager.svelte.ts`, `src/lib/components/InfiniteList.svelte`, unit test `tests/unit/pager.test.ts`.

**Interfaces produced (props):**
- `Hash { value: string; href?: string; head?: number; tail?: number; copy?: boolean (default true) }` — renders `<a class=mono>` or `<span>`, `title=value`, copy button with `aria-label="Copy"`, brief "Copied" state.
- `Amount { nano: string; unit?: 'ERG' | 'nanoERG' (default 'ERG'); maxFrac?: number }` — `title` shows the exact raw nano.
- `Age { ms: number }` — text `relTime`, `title=absTime`, re-renders every 30 s via `$effect` interval; cleans up.
- `Badge { tone?: 'neutral'|'ok'|'warn'|'danger' }` slot content.
- `Panel { title?: string }` slot; `Tabs { tabs: {id,label,count?}[]; active: string; onchange(id) }` keyboard arrows.
- `Table` = thin styled `<table>` wrapper with `dense` rows and sticky head; `EmptyState { message }`; `ErrorState { error: unknown; retry?: () => void }` prints `ApiError.detail` or generic; `Skeleton { rows?: number }`.
- `RentBadge { rent: RentDto; tip: number | null }` → "claimable" (danger) if `claimable_at_tip`, else "matures in N blocks" (warn if N ≤ 720, neutral otherwise); title shows maturity height and due ERG.
- `pager.svelte.ts`:
```ts
export function createPager<T>(fetchPage: (cursor?: string) => Promise<PageDto<T>>) {
  let items = $state<T[]>([]); let cursor = $state<string | null>(null); let loading = $state(false); let error = $state<unknown>(null); let done = $state(false);
  async function loadMore() { if (loading || done) return; loading = true; error = null; try { const p = await fetchPage(cursor ?? undefined); items = [...items, ...p.items]; cursor = p.next_cursor; done = p.next_cursor === null; } catch (e) { error = e; } finally { loading = false; } }
  function reset() { items = []; cursor = null; done = false; error = null; }
  return { get items() { return items; }, get loading() { return loading; }, get error() { return error; }, get done() { return done; }, loadMore, reset };
}
```
- `InfiniteList { pager; children (row snippet); empty?: string }` renders items via snippet, a "Load more" button, and an `IntersectionObserver` sentinel that calls `loadMore()`; shows `Skeleton` while loading first page, `ErrorState` with retry on error, `EmptyState` when done with zero items.

- [ ] **Step 1: Failing test** `tests/unit/pager.test.ts` (runs in node with `$state` via Vitest svelte plugin config — add `@sveltejs/vite-plugin-svelte` test setup or test the pager through a plain-TS twin; decision: implement `createPager` with a small internal store shape testable in node by injecting a `notify` callback if `$state` cannot be used outside components — if `svelte/reactivity` `$state` in `.svelte.ts` works under vitest with the svelte plugin, use it; otherwise fall back to a class with plain fields and a `subscribe`). Test: first `loadMore` appends 2 items and sets cursor; second appends and sets `done` when `next_cursor` is null; error path sets `error` and keeps items; `reset` clears.

- [ ] **Step 2: Run → fail. Step 3: Implement components + pager. Step 4: `npm test && npm run check && npm run lint` pass.**

- [ ] **Step 5: Commit** `feat(frontend): core components (Hash, Amount, Age, Table, InfiniteList, states) and cursor pager`

---

### Task 4: Status badge + `/status` + home page

**Files:** `src/lib/components/StatusBadge.svelte`, `src/lib/status/status.svelte.ts` (polls `/v1/status` every 5 s while the tab is visible; exposes `current: StatusDto | null`, `error`), `src/routes/status/+page.{ts,svelte}`, `src/routes/+page.{ts,svelte}`, wire `StatusBadge` into the layout.

- **StatusBadge**: shows `#indexed` with tone `ok` when `lag_blocks ≤ 3`, `warn` when ≤ 100, `danger` when > 100 or `halted`; `aria-live="polite"`; click → `/status`.
- **/status**: table of indexed, best, lag, mode, source, halted; link `/v1/status` (opens JSON).
- **Home** `+page.ts` loads in parallel: `api.blocks(undefined, 10, fetch)`, `api.txs(undefined, 10, fetch)`, `api.rentUpcoming(720, 5, fetch)`, `api.status(fetch)`. Page: three panels — Latest blocks (height, age, txs, reward, miner truncated), Latest transactions (id, age, fee, outputs count), Rent maturing soon (box value, due, matures in N blocks, address) — plus a lag banner when `lag_blocks > 100 || halted` ("Index is N blocks behind the node").

- [ ] Steps: write the page/badge; `npm run check && npm run lint`; manual dev check against the local explorer; commit `feat(frontend): status polling badge, /status page, home page`.

---

### Task 5: Blocks list and block detail

**Files:** `src/routes/blocks/+page.{ts,svelte}`, `src/routes/blocks/[id]/+page.{ts,svelte}`.

- `/blocks`: `InfiniteList` over `createPager(c => api.blocks(c, 50))`; columns Height (link), Age, Txs, Size (KB), Fees (ERG), Reward (ERG), Miner (Hash of `miner_pk`).
- `/blocks/[id]`: `load` fetches `api.block(id)` and `api.blockTxs(id)` in parallel; on `ApiError 404` → `error(404, 'Block not found')`. Header panel: height, id (Hash, copy), parent (link), timestamp (abs + Age), difficulty, size, version, tx count, fees, reward, miner. Prev/Next buttons link to `height±1`. Tx table: id (link), inputs count, outputs count, total output value (sum of `outputs[].value` with BigInt), fee.

- [ ] Steps: implement; check/lint; manual check with a real block; commit `feat(frontend): blocks list and block detail`.

---

### Task 6: Transactions list and transaction detail

**Files:** `src/routes/txs/+page.{ts,svelte}`, `src/routes/tx/[id]/+page.{ts,svelte}`, `src/lib/components/BoxCard.svelte`, `src/lib/registers/decode.ts` + `tests/unit/registers.test.ts`.

- `decode.ts`: `decodeRegister(hex: string): { type: string; value: string } | null` for the common cases: `0e` + VLQ len → Coll[Byte] shown as UTF‑8 if printable else hex; `04` → Int (zigzag VLQ), `05` → Long (zigzag VLQ), `0400`/`0401` booleans? (no — `01`/`00` are Boolean constants `0100`/`0101`); `07` + 33 bytes → GroupElement hex; `08cd…` → SigmaProp of P2PK. Anything else → `{ type: 'raw', value: hex }`. Unit tests: `0e0568656c6c6f` → `Coll[Byte] "hello"`, `0501` → `Long 0`? careful: zigzag of 1 → -1; test `0502` → `Long 1`, `0400` → `Int 0`.
- `BoxCard { box: BoxDto; tip: number | null; role: 'input'|'output' }`: value (Amount), address (Hash link to `/address/`), box id (Hash link), tokens list (id truncated, amount), registers table (R4–R9 decoded), `RentBadge`, spent-by link when `spent_by`.
- `/tx/[id]`: two columns Inputs (BoxCard or "unknown box" for `box === null`) → Outputs; header facts: id, block (link), timestamp, size, fee, data inputs (Hash links to `/box/`). Totals row: sum in, sum out, fee.
- `/txs`: `InfiniteList` newest first: id, block, age, inputs/outputs count, fee.

- [ ] Steps: failing register tests → implement → components/pages → check/lint/test → commit `feat(frontend): transactions list, transaction detail with box cards and register decoding`.

---

### Task 7: Box detail

**Files:** `src/routes/box/[id]/+page.{ts,svelte}`.

- Facts: id, value, address, created in tx (link) at index, creation height (link to block), size, ergo tree (collapsible hex, template hash), tokens, registers (decoded), status: Unspent / Spent by tx (link) at height (link). Rent panel: maturity height, due rent (Amount), "claimable now" or "matures in N blocks (≈ D days at 2 min/block)", plus a note when spent ("rent no longer applies").

- [ ] Steps: implement; check/lint; manual check; commit `feat(frontend): box detail with rent panel`.

---

### Task 8: Address page

**Files:** `src/routes/address/[addr]/+page.{ts,svelte}`.

- `load`: `api.address(addr)`; 404 → page renders "Address not seen yet" state (not an error page) with the address echoed.
- Header: address (Hash full, copy), balance (Amount, large), tokens (count + list), box count, first/last seen (block links).
- Tabs (URL hash `#txs|#unspent|#boxes|#rent`, default txs): Transactions (`createPager(c => api.addressTxs(addr, c, 50))`: id, block, age, fee), Unspent boxes (`addressBoxes(addr, true, …)`: id, value, created, tokens, RentBadge), All boxes (`unspent=false`: adds spent column), Rent (`api.addressRent(addr)`: sorted by maturity asc; shows "truncated" notice when `truncated`; columns id, value, due, maturity height, "in N blocks").

- [ ] Steps: implement; check/lint; manual check; commit `feat(frontend): address page with transactions/unspent/boxes/rent tabs`.

---

### Task 9: Rich list and rent pages

**Files:** `src/routes/richlist/+page.{ts,svelte}`, `src/routes/rent/+page.{ts,svelte}`, `src/lib/format/supply.ts` + `tests/unit/supply.test.ts`.

- `supply.ts`: `circulatingAt(height: number): bigint` — Ergo emission schedule: 75 ERG/block for the first 525,600 blocks, then decreasing by 3 ERG every 64,800 blocks down to 3 ERG (until EIP‑27 adjustments; document that this is the pre‑EIP‑27 formula and treat as approximate — label the column "≈ % of supply"). Unit tests for heights 1, 525,600, 590,400.
- `/richlist`: rank, address (Hash link; tree hash fallback when `address` null), balance, ≈ % of supply. Pager over `api.richlist`.
- `/rent`: tabs Upcoming / Eligible. Upcoming: N selector (72 / 720 / 2160 blocks) → `api.rentUpcoming(N, 100)`; table: matures at (height + "in N blocks"), box (link), value, due rent, address. Eligible: pager over `api.rentEligible`; same columns with "claimable" badge. Summary line: count + total due ERG (BigInt sum).

- [ ] Steps: failing supply tests → implement → pages → check/lint/test → commit `feat(frontend): rich list and rent pages`.

---

### Task 10: Global search

**Files:** `src/lib/components/SearchBox.svelte`, `src/routes/search/+page.ts`, `src/routes/search/+page.svelte` (not-found view), layout wiring, `tests/unit/classify.test.ts` (extend with routing table).

- `SearchBox`: input with placeholder "Height, block, tx, box or address"; on submit → `goto('/search?q=' + encodeURIComponent(q))`; global keydown: `/` focuses (unless in an input), `Esc` blurs/clears.
- `/search/+page.ts`: `classify(q)`: height → `redirect(302, '/blocks/'+value)`; address → `redirect(302, '/address/'+value)`; hex32 → `api.search(q)` → redirect by `kind` (`block→/blocks/{id}`, `tx→/tx/{id}`, `box→/box/{id}`); `ApiError 404` or unknown → return `{ q, reason }` for the not-found page (hints: "64-hex ids can be block, transaction or box ids; addresses start with 9").
- Add pure `routeFor(c: Classified): string | null` in `classify.ts` with tests.

- [ ] Steps: tests → implement → check/lint/test → commit `feat(frontend): global search with keyboard shortcuts and resolver route`.

---

### Task 11: Mock API, Playwright e2e, budget in CI

**Files:** `tests/e2e/mock/fixtures.ts` (reads `../../../tests/fixtures/blocks/*.json`, converts to DTO JSON: blocks, txs, boxes, one address aggregate, richlist from balances, rent from unspent outputs), `tests/e2e/mock/handlers.ts` (MSW handlers for every `/v1` route incl. problem JSON 404s), `tests/e2e/mock/server.ts` (a tiny Node HTTP server using the handlers on port 18099, started by Playwright `webServer`), `playwright.config.ts` (webServer runs `vite build && vite preview --port 4173` with `VITE_API_PROXY=http://127.0.0.1:18099`, plus the mock server), `tests/e2e/{home,blocks,tx,box,address,richlist,rent,search,theme,errors}.spec.ts`.

Assertions: each route renders its heading and at least one row; `/blocks` loads a second page when scrolled (mock paginates by 5); search flows for height/tx/box/address/unknown; theme toggle persists across reload; `/` key focuses search; 500 from mock → `ErrorState` with retry; lag banner appears when mock status `lag_blocks=500`.

Note: `vite.config.ts` proxy target must read `process.env.VITE_API_PROXY ?? 'http://127.0.0.1:18090'`.

- [ ] Steps: fixtures/handlers → specs → `npx playwright install chromium` → `npm run test:e2e` green → commit `test(frontend): MSW mock API from fixtures and Playwright e2e for every route`.

---

### Task 12: Deploy script, Caddy update, first deploy

**Files:** `scripts/deploy_frontend.sh` (repo root), Caddyfile change on Nuremberg, `frontend/README.md`.

`scripts/deploy_frontend.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
HOST="${1:-$DEPLOY_HOST}"; KEY="${SSH_KEY:-}"
cd "$(dirname "$0")/../frontend"
npm ci --silent && npm run build
rsync -az --delete -e "ssh -i $KEY -o BatchMode=yes" build/ "$HOST:/var/www/explorer/"
ssh -i "$KEY" -o BatchMode=yes "$HOST" 'systemctl reload caddy && curl -s -o /dev/null -w "%{http_code}\n" https://explorer.kadia.io/'
```
Caddyfile (Nuremberg `/etc/caddy/Caddyfile`):
```
explorer.kadia.io {
	encode zstd gzip
	handle /v1/* { reverse_proxy 127.0.0.1:18090 }
	handle {
		root * /var/www/explorer
		@immutable path /_app/immutable/*
		header @immutable Cache-Control "public, max-age=31536000, immutable"
		header /index.html Cache-Control "no-cache"
		try_files {path} /index.html
		file_server
	}
}
```
- [ ] Steps: write script + README (dev, test, deploy, budget); update Caddyfile on Nuremberg (`caddy validate` then reload); run the deploy; verify `https://explorer.kadia.io/`, `/blocks`, a deep link `/blocks/1000` served by SPA fallback (200 + app shell) and `/v1/status` still proxied; commit `feat(frontend): deploy script, Caddy SPA fallback with immutable asset caching, README`.

---

## Self-review

- **Spec coverage:** §2 stack/styling/theme/deploy/repo/testing → T1, T11, T12 (TanStack replaced per the header amendment); §3 routes → T4 (home, status), T5, T6, T7, T8, T9, T10; §4 shell/tokens/primitives/density/a11y → T1, T3; §5 client/types/BigInt/prefetch (native preload) /pagination → T2, T3; §6 classifier + keyboard → T2, T10; §7 error handling (ErrorState, lag banner, 404 page) → T3, T4, T1 (`+error.svelte`); §8 build/deploy/Caddy/budget → T1, T12; §9 tests → T2, T3, T6, T9, T10, T11.
- **Placeholders:** none; every task names files, props/signatures, and tests.
- **Type consistency:** `PageDto.next_cursor: string | null` used by `createPager`; `api.*` signatures take `(…, fetchFn?)` so `load` can pass SvelteKit's `fetch`; `RentDto`/`BoxDto` shapes match the Rust DTOs (`InputDto.box` is the serde‑renamed field); `classify` kinds match `routeFor`.
