# sv

Everything you need to build a Svelte project, powered by [`sv`](https://github.com/sveltejs/cli).

## Creating a project

If you're seeing this, you've probably already done this step. Congrats!

```sh
# create a new project
npx sv create my-app
```

To recreate this project with the same configuration:

```sh
# recreate this project
npx sv@0.17.0 create --template minimal --types ts --install npm frontend
```

## Developing

Once you've created a project and installed dependencies with `npm install` (or `pnpm install` or `yarn`), start a development server:

```sh
npm run dev

# or start the server and open the app in a new browser tab
npm run dev -- --open
```

## Building

To create a production version of your app:

```sh
npm run build
```

You can preview the production build with `npm run preview`.

> To deploy your app, you may need to install an [adapter](https://svelte.dev/docs/kit/adapters) for your target environment.

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
   specs: `x-mock-fail: 500` makes every `/v1` route answer with problem JSON, and
   `x-mock-lag: 500` makes `/v1/status` report a lagging index. Both also work as `?__fail=`
   / `?__lag=` query parameters when poking at the mock by hand.
2. `npm run build && vite preview --port 4173`, with `VITE_API_PROXY` pointed at the mock so
   the preview server proxies `/v1` to it (see `preview.proxy` in `vite.config.ts`).

Install the browser once before the first run — no root needed, it lands in
`~/.cache/ms-playwright`:

```sh
npx playwright install chromium
```

The specs in `tests/e2e/*.spec.ts` cover every phase-1 route: home, blocks list (including
cursor pagination as the infinite-scroll sentinel comes into view), block detail,
transaction, box (rent panel), address (both the populated and the "not seen yet" state),
rich list, both rent tabs, the search flows for a height / tx id / box id / block id /
address / unparseable query, the theme toggle persisting across a reload, the `/` search
shortcut, an API 500 rendering `ErrorState` with a working Retry, and the lag banner.

The home route's gzipped-JS budget runs automatically after every `npm run build`
(`npm run budget`, 120 KB).
