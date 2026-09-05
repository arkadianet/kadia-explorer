// Fails the build when the home route's gzipped JS exceeds the budget.
//
// Simplest correct approach: SvelteKit's static client build always loads the
// entry chunk(s), the shared chunks they import, and per-route "node" modules
// (nodes/0.*.js = root layout, nodes/2.*.js = the home route, "/", since
// node 1 is the universal +error fallback). We gzip all of those and sum.
import { gzipSync } from 'node:zlib';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const BUDGET_BYTES = 120 * 1024;
const IMMUTABLE_DIR = join('build', '_app', 'immutable');

function listJsFiles(dir) {
	try {
		return readdirSync(dir)
			.filter((f) => f.endsWith('.js'))
			.map((f) => join(dir, f));
	} catch {
		return [];
	}
}

function matchesHomeNode(filename) {
	// nodes/0.<hash>.js = root +layout, nodes/2.<hash>.js = home +page
	return /^(0|2)\..*\.js$/.test(filename);
}

const entryFiles = listJsFiles(join(IMMUTABLE_DIR, 'entry'));
const chunkFiles = listJsFiles(join(IMMUTABLE_DIR, 'chunks'));
const nodeFiles = listJsFiles(join(IMMUTABLE_DIR, 'nodes')).filter((f) =>
	matchesHomeNode(f.split('/').pop() ?? '')
);

const files = [...entryFiles, ...chunkFiles, ...nodeFiles];

if (files.length === 0) {
	console.error(`bundle-budget: no JS files found under ${IMMUTABLE_DIR} — did the build run?`);
	process.exit(1);
}

let total = 0;
const rows = [];

for (const file of files) {
	const raw = readFileSync(file);
	const gzipped = gzipSync(raw);
	total += gzipped.length;
	rows.push({
		file,
		raw: raw.length,
		gzip: gzipped.length
	});
}

rows.sort((a, b) => b.gzip - a.gzip);

const fmt = (n) => `${(n / 1024).toFixed(2)} KB`;

console.log('\nHome route bundle budget (entry + chunks + layout/home nodes):\n');
console.log(`${'file'.padEnd(60)} raw       gzip`);
for (const row of rows) {
	console.log(`${row.file.padEnd(60)} ${fmt(row.raw).padEnd(9)} ${fmt(row.gzip)}`);
}
console.log('\n' + '-'.repeat(80));
console.log(`Total gzipped: ${fmt(total)} (budget: ${fmt(BUDGET_BYTES)})\n`);

if (total > BUDGET_BYTES) {
	console.error(
		`bundle-budget: FAILED — ${fmt(total)} exceeds the ${fmt(BUDGET_BYTES)} budget for the home route.`
	);
	process.exit(1);
}

console.log('bundle-budget: OK');
