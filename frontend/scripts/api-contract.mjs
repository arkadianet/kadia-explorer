// Build the documented SDK subset from the same wire types used by the explorer.
// This is a declaration-to-schema converter, not runtime response validation.
import ts from 'typescript';
import { format, resolveConfig } from 'prettier';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { operations, sourceFiles } from './api-contract-catalog.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const definitions = new Map();
for (const file of sourceFiles) {
	const source = ts.createSourceFile(
		file,
		readFileSync(resolve(root, file), 'utf8'),
		ts.ScriptTarget.Latest,
		true
	);
	for (const node of source.statements) {
		if (ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node))
			definitions.set(node.name.text, node);
	}
}
const components = {};
const used = new Set();
function named(name, args = []) {
	const node = definitions.get(name);
	if (!node) throw new Error(`Unknown wire type: ${name}`);
	used.add(name);
	const bindings = new Map((node.typeParameters ?? []).map((p, i) => [p.name.text, args[i]]));
	if (args.length) return declaration(node, bindings);
	if (!(name in components)) {
		components[name] = {};
		components[name] = declaration(node, bindings);
	}
	return { $ref: `#/components/schemas/${name}` };
}
function object(members, bindings) {
	const properties = {},
		required = [];
	let additionalProperties;
	for (const member of members) {
		if (ts.isPropertySignature(member)) {
			const key = member.name.getText().replace(/^['"]|['"]$/g, '');
			properties[key] = schema(member.type, bindings);
			if (!member.questionToken) required.push(key);
		} else if (ts.isIndexSignatureDeclaration(member))
			additionalProperties = schema(member.type, bindings);
		else throw new Error(`Unsupported member: ${member.getText()}`);
	}
	return {
		type: 'object',
		properties,
		...(required.length ? { required } : {}),
		...(additionalProperties !== undefined ? { additionalProperties } : {})
	};
}
function declaration(node, bindings) {
	if (ts.isTypeAliasDeclaration(node)) return schema(node.type, bindings);
	const own = object(node.members, bindings);
	if (!node.heritageClauses) return own;
	return {
		allOf: [
			...node.heritageClauses.flatMap((c) =>
				c.types.map((t) =>
					named(
						t.expression.getText(),
						(t.typeArguments ?? []).map((a) => schema(a, bindings))
					)
				)
			),
			own
		]
	};
}
function schema(node, bindings = new Map()) {
	if (!node) throw new Error('Missing type');
	if (node.kind === ts.SyntaxKind.StringKeyword) return { type: 'string' };
	if (node.kind === ts.SyntaxKind.NumberKeyword) return { type: 'number' };
	if (node.kind === ts.SyntaxKind.BooleanKeyword) return { type: 'boolean' };
	if (node.kind === ts.SyntaxKind.UnknownKeyword) return {};
	if (ts.isParenthesizedTypeNode(node)) return schema(node.type, bindings);
	if (ts.isLiteralTypeNode(node)) {
		if (node.literal.kind === ts.SyntaxKind.NullKeyword) return { type: 'null' };
		const value = ts.isStringLiteral(node.literal)
			? node.literal.text
			: JSON.parse(node.literal.getText());
		return { type: typeof value, const: value };
	}
	if (ts.isUnionTypeNode(node)) return { anyOf: node.types.map((t) => schema(t, bindings)) };
	if (ts.isArrayTypeNode(node)) return { type: 'array', items: schema(node.elementType, bindings) };
	if (ts.isTypeLiteralNode(node)) return object(node.members, bindings);
	if (ts.isIndexedAccessTypeNode(node)) {
		const owner = definitions.get(node.objectType.getText());
		const key = node.indexType.getText().replace(/^['"]|['"]$/g, '');
		const field = owner?.members?.find((m) => m.name?.getText() === key);
		if (!field) throw new Error(`Unsupported indexed access ${node.getText()}`);
		return schema(field.type, bindings);
	}
	if (ts.isTypeReferenceNode(node)) {
		const name = node.typeName.getText();
		if (bindings.has(name)) return bindings.get(name);
		if (name === 'Record')
			return { type: 'object', additionalProperties: schema(node.typeArguments[1], bindings) };
		return named(
			name,
			(node.typeArguments ?? []).map((a) => schema(a, bindings))
		);
	}
	throw new Error(`Unsupported wire syntax: ${node.getText()}`);
}
const paths = {};
for (const [name, op] of Object.entries(operations)) {
	const parsed = ts.createSourceFile(
		'response.ts',
		`type Response = ${op.response};`,
		ts.ScriptTarget.Latest,
		true
	);
	const response = schema(parsed.statements[0].type);
	const parameters = Object.entries(op.parameters)
		.filter(() => op.method !== 'POST')
		.map(([key, value]) => ({
			name: key,
			in: op.path.includes(`{${key}}`) ? 'path' : 'query',
			required: op.required.includes(key),
			schema: value
		}));
	if (op.paged)
		parameters.push({
			name: 'consistency',
			in: 'query',
			schema: {
				type: 'string',
				enum: op.strictOnly ? ['strict'] : ['strict', 'best_effort'],
				default: op.strictOnly ? 'strict' : 'best_effort'
			},
			description:
				'The SDK always requests strict consistency and carries cursor and snapshot together.'
		});
	paths[op.path] = {
		[op.method === 'POST' ? 'post' : 'get']: {
			operationId: name,
			summary: name.replace(/[A-Z]/g, (c) => ` ${c.toLowerCase()}`),
			parameters,
			...(op.method === 'POST'
				? {
						requestBody: {
							required: true,
							content: {
								'application/json': {
									schema: {
										type: 'object',
										required: op.required,
										properties: op.parameters,
										additionalProperties: false
									}
								}
							}
						}
					}
				: {}),
			responses: {
				200: {
					description:
						'Observation. Read its source, completeness, timestamps and snapshot fields. Mempool entries are pending on one configured node; they are not indexed confirmation, ownership or network-wide acceptance claims.',
					headers: {
						'x-explorer-completeness': {
							schema: { type: 'string', enum: ['complete', 'incomplete'] },
							description: 'Coverage of the store read. Omitted for source-only/status responses.'
						}
					},
					content: { 'application/json': { schema: response } }
				},
				default: {
					description:
						'HTTP failures include invalid input (400), missing data (404), timeout (408), changed snapshot (409), body limit (413), read budget (422), throttling (429), unavailable history/source (503), or integrity failure (500). Framework errors may have no JSON body. The SDK preserves HTTP status and does not automatically retry.',
					content: {
						'application/problem+json': {
							schema: {
								type: 'object',
								properties: {
									type: { type: 'string' },
									title: { type: 'string' },
									status: { type: 'integer' },
									detail: { type: 'string' },
									code: { type: 'string' }
								},
								required: ['title', 'status', 'detail']
							}
						}
					}
				}
			}
		}
	};
}
const spec = {
	openapi: '3.1.1',
	info: {
		title: 'Kadia explorer workflow API',
		version: '1.0.0',
		description: `Versioned documentation for the ${Object.keys(operations).length} operations supported by the portable SDK. Other server routes are outside this contract subset. Amounts and difficulty remain decimal strings; heights/counts are numbers. Exact historical address queries require full retained history; network history requires the entire requested range to be retained. Contract version is distinct from chain height and software release. Compatible fields may be added; unknown enum values should be surfaced rather than guessed.`
	},
	servers: [{ url: '/v1' }],
	paths,
	components: { schemas: components }
};
const outputs = {
	'static/openapi.json': JSON.stringify(spec, null, 2) + '\n',
	'static/sdk/operations.js':
		'// Generated by scripts/api-contract.mjs. Do not edit.\nexport const operations = ' +
		JSON.stringify(operations, null, 2) +
		';\n',
	'static/sdk/api-types.d.ts':
		'// Generated from explorer wire types. Amounts stay decimal strings.\n' +
		[...used].map((name) => definitions.get(name).getText()).join('\n\n') +
		'\n'
};
// Pull types referenced only by declarations into the generated SDK module as well.
let grew = true;
while (grew) {
	grew = false;
	for (const name of [...used]) {
		const visit = (node) => {
			if (
				ts.isTypeReferenceNode(node) &&
				definitions.has(node.typeName.getText()) &&
				!used.has(node.typeName.getText())
			) {
				used.add(node.typeName.getText());
				grew = true;
			}
			ts.forEachChild(node, visit);
		};
		visit(definitions.get(name));
	}
}
outputs['static/sdk/api-types.d.ts'] =
	'// Generated from explorer wire types. Amounts stay decimal strings.\n' +
	[...used].map((name) => definitions.get(name).getText()).join('\n\n') +
	'\n';
const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });
// Normalize declarations through TypeScript; source files may have Windows line endings.
outputs['static/sdk/api-types.d.ts'] = printer.printFile(
	ts.createSourceFile(
		'api-types.d.ts',
		outputs['static/sdk/api-types.d.ts'],
		ts.ScriptTarget.Latest,
		true
	)
);
function parameterType(value) {
	if (value.anyOf) return value.anyOf.map(parameterType).join(' | ');
	if (value.enum) return value.enum.map((v) => JSON.stringify(v)).join(' | ');
	if (value.type === 'array') return `(${parameterType(value.items)})[]`;
	return value.type === 'integer' ? 'number' : value.type;
}
outputs['static/sdk/api-types.d.ts'] +=
	'\nexport interface KadiaOperations {\n' +
	Object.entries(operations)
		.map(
			([key, op]) =>
				`${key}: { parameters: ${
					Object.keys(op.parameters).length
						? `{ ${Object.entries(op.parameters)
								.map(
									([p, value]) =>
										`${p}${op.required.includes(p) ? '' : '?'}: ${parameterType(value)}`
								)
								.join('; ')} }`
						: 'Record<string, never>'
				}; response: ${op.response} };`
		)
		.join('\n') +
	'\n}\nexport type PagedOperation = ' +
	Object.entries(operations)
		.filter(([, op]) => op.paged)
		.map(([key]) => JSON.stringify(key))
		.join(' | ') +
	';\n';
const prettier = await resolveConfig(resolve(root, 'package.json'));
for (const [file, value] of Object.entries(outputs)) {
	const path = resolve(root, file);
	const formatted = await format(value, { ...prettier, filepath: path });
	if (process.argv.includes('--check')) {
		if (readFileSync(path, 'utf8').replaceAll('\r\n', '\n') !== formatted)
			throw new Error(`${file} is stale. Run npm run contract:generate.`);
	} else {
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, formatted);
	}
}
console.log(
	`API contract: ${Object.keys(operations).length} operations, ${Object.keys(components).length} schemas checked.`
);
