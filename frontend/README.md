# Ergo Explorer — frontend

The SvelteKit single-page app for the standalone Ergo explorer. It talks to `xp-api` over
`/v1` and ships as a static bundle (`@sveltejs/adapter-static`, `ssr = false`).

## Developing

Install dependencies with `npm install`, then start the dev server:

```sh
npm run dev

# or start the server and open the app in a new browser tab
npm run dev -- --open
```

`vite dev` (and `vite preview`) proxy `/v1` to the explorer API at `http://127.0.0.1:18090` by
default. Point it elsewhere with `VITE_API_PROXY`:

```sh
VITE_API_PROXY=http://127.0.0.1:18090 npm run dev
```

## Building

To create a production version of your app:

```sh
npm run build
```

You can preview the production build with `npm run preview`.

## Testing

Two layers, both run from this directory.

**Unit tests** (`npm test`) — Vitest over `tests/unit/**`, covering the pure modules:
amount/size/hash/time formatting, register decoding, search classification, the cursor
pager and the API client. No browser, no network.

**End-to-end tests** (`npm run test:e2e`) — Playwright (chromium only) drives the built app
against a mock API. `playwright.config.ts` starts two servers itself:

1. `node tests/e2e/mock/server.ts` on port 18099 — a dependency-free `node:http` router
   (`tests/e2e/mock/handlers.ts`) over a dataset built from the repo's committed block
   fixtures at `tests/fixtures/blocks/*.json` (`tests/e2e/mock/fixtures.ts`, which documents
   where the mock deliberately differs from `xp-api`). Node ≥ 22.18 strips the TypeScript
   types, so nothing compiles it first. Two request headers steer it, for the error-state
   specs: `x-mock-fail: 500` makes every `/v1` route answer with problem JSON,
   `x-mock-lag: 500` makes `/v1/status` report a lagging index, and `x-mock-stall: 1` makes
   it report a non-null `stalled`. All three also work as `?__fail=` / `?__lag=` / `?__stall=`
   query parameters when poking at the mock by hand.
2. `npm run build && vite preview --port 4173`, with `VITE_API_PROXY` pointed at the mock so
   the preview server proxies `/v1` to it (see `preview.proxy` in `vite.config.ts`).

Neither server is ever reused (`reuseExistingServer: false`), so `npm run test:e2e` always
rebuilds and always rebuilds the mock's dataset — a preview left running from an earlier
session can never make the suite test a stale bundle. Both ports (4173, 18099) must be free.

Install the browser once before the first run — no root needed, it lands in
`~/.cache/ms-playwright`:

```sh
npx playwright install chromium
```

The specs in `tests/e2e/*.spec.ts` cover every phase-1 route:

- `/` (home) — the three panels; `/blocks` and `/txs` — the lists, each pulling in further
  pages as the infinite-scroll sentinel comes into view
- `/blocks/[id]`, `/tx/[id]`, `/box/[id]` — detail pages, including the box rent panel, plus
  the 404 error page for each
- `/address/[addr]` — both the populated page (with its hash-driven tabs) and the
  "Address not seen yet" state
- `/richlist`, `/rent` (upcoming and eligible tabs), `/status` (fields, and the stall callout
  under `x-mock-stall`)
- `/search` — the flows for a height / tx id / box id / block id / address / unparseable
  query / unknown id

plus the cross-cutting behaviour: the theme toggle persisting across a reload, the `/` search
shortcut, an API 500 rendering `ErrorState` with a working Retry, and the lag banner.

## Bundle budget

`npm run build` runs `postbuild` → `npm run budget` (`scripts/bundle-budget.mjs`), which gzips
the root layout, the home route, and their shared entry/chunk files under
`build/_app/immutable/` and fails the build if the total exceeds 120 KB. Run it standalone
against an existing `build/` with `npm run budget`.

## Deploy

`scripts/deploy_frontend.sh` (repo root) builds and ships this app to the production host
(Nuremberg, `explorer.kadia.io`):

```sh
scripts/deploy_frontend.sh [--caddy] [user@host] [site-url]
```

It:

1. `cd`s into `frontend/`, runs `npm ci --silent && npm run build` (which also runs the
   bundle budget check as `postbuild`).
2. `rsync -az --delete`s `build/` to `/var/www/explorer/` over SSH, so removed files are
   pruned on the host too.
3. With `--caddy`, scp's `deploy/caddy/explorer.kadia.io.Caddyfile` to the host, runs
   `caddy validate` on it there, backs the live config up as `/etc/caddy/Caddyfile.bak.<ts>`
   and installs it. Without the flag the host keeps the config it already has.
4. Reloads Caddy on the host and prints the resulting HTTPS status code for `$SITE_URL`.

Defaults to `root@167.233.240.191`; pass a different `user@host` as the first positional
argument to target another box, and that box's own site URL as the second (or via the
`SITE_URL` environment variable — default `https://explorer.kadia.io`) so the health check
follows the host. Set `SSH_KEY` to override the default key (`~/.ssh/hetzner_vps`).

The committed site config (`deploy/caddy/explorer.kadia.io.Caddyfile`) reverse-proxies `/v1/*`
to the explorer API on `127.0.0.1:18090`, serves the built SPA from `/var/www/explorer` with a
`try_files` fallback to `index.html` for client-side routes, sets a one-year immutable
`Cache-Control` on `/_app/immutable/*`, and `no-cache` on **every other** response — the SPA
shell included, so a client-side route like `/blocks/1000` can never be served from a stale
cached shell after a deploy.

Prerequisites: SSH access to the host with the deploy key, and `caddy` already running there
under systemd (the script reloads it, it does not install or start it).
