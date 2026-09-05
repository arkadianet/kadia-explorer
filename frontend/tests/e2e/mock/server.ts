/**
 * Standalone HTTP server for the e2e mock API. Playwright's `webServer` starts it with
 * `node tests/e2e/mock/server.ts` — Node ≥ 22.18 strips the TypeScript types itself, so no
 * loader, bundler or extra dependency is involved.
 *
 * Run it by hand to poke at the mock:
 *   node tests/e2e/mock/server.ts
 *   curl -s localhost:18099/v1/blocks | jq
 */

import { createServer } from 'node:http';
import { handle } from './handlers.ts';

export const MOCK_PORT = Number(process.env.MOCK_PORT ?? 18099);

const server = createServer((req, res) => {
	try {
		handle(req, res);
	} catch (e) {
		if (!res.headersSent) {
			res.writeHead(500, { 'content-type': 'application/problem+json' });
		}
		res.end(
			JSON.stringify({
				type: 'about:blank',
				title: 'Internal Server Error',
				status: 500,
				detail: e instanceof Error ? e.message : String(e)
			})
		);
	}
});

server.listen(MOCK_PORT, '127.0.0.1', () => {
	console.log(`mock api listening on http://127.0.0.1:${MOCK_PORT}`);
});

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
	process.on(signal, () => server.close(() => process.exit(0)));
}
