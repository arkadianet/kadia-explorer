// Node 22+ example: node inspect-transaction.mjs <64-hex transaction ID> [API root]
// Keep this file beside kadia-client.js and operations.js. No package install is needed.
import { createKadiaClient, KadiaApiError } from './kadia-client.js';

const [id, baseUrl = 'https://explorer.kadia.io/v1'] = process.argv.slice(2);
if (!id || !/^[a-fA-F0-9]{64}$/.test(id)) {
	console.error('Usage: node inspect-transaction.mjs <64-hex transaction ID> [API root]');
	process.exitCode = 1;
} else {
	try {
		const client = createKadiaClient({ baseUrl });
		const observation = await client.observe('transactionStatus', { id });
		console.log(JSON.stringify(observation, null, 2));
		if (observation.data.state === 'confirmed') {
			const detail = await client.observe('transaction', { id });
			// These are separate reads. Do not combine incompatible inclusions after a reorg.
			if (!detail.data.block_id) {
				throw new Error('The server does not expose the detail block ID.');
			}
			if (
				!observation.data.inclusion ||
				detail.data.block_id !== observation.data.inclusion.block_id
			) {
				throw new Error('Inclusion changed between reads; restart the inspection.');
			}
			console.log(
				JSON.stringify(
					{
						id: detail.data.id,
						block_id: detail.data.block_id,
						fee_nano: detail.data.fee, // exact decimal string; not Number(...)
						completeness: detail.completeness,
						inputs: detail.data.inputs.length,
						outputs: detail.data.outputs.length
					},
					null,
					2
				)
			);
		}
	} catch (error) {
		console.error(
			error instanceof KadiaApiError
				? `HTTP ${error.status} (${error.code ?? 'unspecified'}): ${error.message}`
				: String(error)
		);
		process.exitCode = 1;
	}
}
