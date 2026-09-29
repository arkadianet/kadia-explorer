<script lang="ts">
	import type { TxStatusDto } from '$lib/api/types';
	import { validatePendingDetails } from '$lib/tx/pending';
	import { formatErg, formatNano } from '$lib/format/amount';
	import { absTime } from '$lib/format/time';
	import { truncateMiddle } from '$lib/format/hash';
	import Hash from './Hash.svelte';
	let { observation, stale = false }: { observation: TxStatusDto; stale?: boolean } = $props();
	const pending = $derived(observation.pending);
	const details = $derived(
		pending?.details ? validatePendingDetails(pending.details, pending) : null
	);
	let expanded = $state({ inputs: false, outputs: false, data: false });
	// The route keys this component by transaction ID. Polling the same ID must
	// preserve a user's locally expanded evidence while navigation resets it.
</script>

<section
	class="pending-details"
	class:available={!!details}
	aria-label="Pending transaction details"
>
	{#if !details}<h2>Pending details</h2>{/if}
	{#if stale || observation.mempool.observation !== 'present'}<p class="coverage" role="status">
			These details are from the last successful pending observation{observation.mempool
				.last_seen_at_ms !== null
				? ` (${absTime(observation.mempool.last_seen_at_ms)})`
				: ''}. They do not establish the transaction’s current status.
		</p>{/if}
	{#if !pending?.details}<p class="coverage" role="status">
			This server provides pending summary counts only. Input and output details are unavailable
			until it supports pending details or indexes the transaction.
		</p>
	{:else if !details}<p class="coverage" role="status">
			Pending details could not be interpreted safely. The observation summary above remains
			available.
		</p>
	{:else}
		{#if !details.complete}<p class="coverage" role="status">
				Limited observation. Missing or omitted details are labelled below.
			</p>{/if}
		<div class="pending-flow">
			<section class="pending-outputs" aria-label="Pending outputs">
				<h2>
					Outputs <span>{details.outputs.length} of {pending.output_count} declared outputs</span>
				</h2>
				<div class="outputs">
					{#each details.outputs.slice(0, expanded.outputs ? undefined : 4) as output (output.index)}<article
							aria-label={'Pending output ' + (output.index + 1)}
						>
							<div class="output-heading">
								<strong>Output {output.index + 1}</strong><span
									class="output-value"
									title={output.value + ' nanoERG'}>{formatErg(output.value)} ERG</span
								>
							</div>
							{#if output.id}<a
									class="output-id"
									href={'/box/' + output.id}
									aria-label={'Output box ' + output.id}>{output.id}</a
								>{:else}<p class="unknown-values">Output box ID not supplied</p>{/if}
							{#if output.address}<p class="output-address">
									<span>Derived output address</span><a
										href={'/address/' + output.address}
										aria-label={output.address}
										title={output.address}>{truncateMiddle(output.address, 12, 8)}</a
									>
								</p>{/if}
							{#if output.token_count === null}<p class="unknown-values">
									Token entries not supplied; token contents are unknown.
								</p>{:else if output.token_count === 0}<p class="token-empty">
									No token entries in this output.
								</p>{:else}<ul
									class="output-tokens"
									aria-label={'Tokens in pending output ' + (output.index + 1)}
								>
									{#each output.tokens as token (token.id)}<li>
											<a href={'/token/' + token.id} aria-label={'Token ' + token.id}>{token.id}</a
											><strong>{formatNano(token.amount)} raw units</strong>
										</li>{/each}
								</ul>{/if}
							{#if output.tokens_truncated}<p class="coverage">
									Showing {output.tokens.length} of {output.token_count} token entries.
								</p>{/if}
							<details class="output-evidence">
								<summary>Exact value and script</summary>
								<dl>
									<div>
										<dt>Value</dt>
										<dd>{formatNano(output.value)} nanoERG</dd>
									</div>
									{#if output.address}<div>
											<dt>Derived output address</dt>
											<dd>{output.address}</dd>
											<dd class="script-copy">
												<Hash
													value={output.address}
													copyLabel={`Copy output ${output.index + 1} address`}
												/>
											</dd>
										</div>{/if}
									<div>
										<dt>ErgoTree</dt>
										<dd>
											{output.ergo_tree ?? 'Script omitted because it exceeds the detail limit.'}
										</dd>
									</div>
								</dl>
								{#if output.ergo_tree}<div class="script-copy">
										<Hash
											value={output.ergo_tree}
											copyLabel={`Copy output ${output.index + 1} ErgoTree`}
										/>
									</div>{/if}
							</details>
						</article>{/each}
				</div>
				{#if details.outputs.length > 4}<button
						type="button"
						onclick={() => (expanded.outputs = !expanded.outputs)}
						>{expanded.outputs
							? 'Show fewer pending outputs'
							: `Show all ${details.outputs.length} loaded pending outputs`}</button
					>{/if}
				{#if details.outputs_truncated}<p class="coverage">
						Additional outputs were omitted. Listed values are not the complete output total.
					</p>{/if}
			</section>
			<section class="pending-inputs" aria-label="Pending inputs">
				<h2>Inputs <span>{details.inputs.length} of {pending.input_count} references</span></h2>
				<p class="unknown-values">Input values and assets: unknown</p>
				<ol>
					{#each details.inputs.slice(0, expanded.inputs ? undefined : 4) as id (id)}<li>
							<a href={'/box/' + id} aria-label={'Input box ' + id}>{id}</a><span
								>Unresolved input reference</span
							>
						</li>{/each}
				</ol>
				{#if details.inputs.length > 4}<button
						type="button"
						onclick={() => (expanded.inputs = !expanded.inputs)}
						>{expanded.inputs
							? 'Show fewer input references'
							: `Show all ${details.inputs.length} loaded input references`}</button
					>{/if}
				{#if details.inputs_truncated}<p class="coverage">
						Additional input references were omitted.
					</p>{/if}
				{#if pending.data_input_count > 0}<details class="data-references">
						<summary
							>Read-only data inputs ({details.data_inputs.length} of {pending.data_input_count})</summary
						>
						<ul>
							{#each details.data_inputs.slice(0, expanded.data ? undefined : 4) as id (id)}<li>
									<a href={'/box/' + id} aria-label={'Data input box ' + id}>{id}</a>
								</li>{/each}
						</ul>
						{#if details.data_inputs.length > 4}<button
								type="button"
								onclick={() => (expanded.data = !expanded.data)}
								>{expanded.data
									? 'Show fewer data inputs'
									: `Show all ${details.data_inputs.length} loaded data inputs`}</button
							>{/if}{#if details.data_inputs_truncated}<p class="coverage">
								Additional data-input references were omitted.
							</p>{/if}
					</details>{/if}
			</section>
		</div>
		<p class="detail-scope">
			Token quantities are exact raw units; names and decimal places are not resolved. Box and token
			links open the explorer’s indexed records, which may not yet exist for pending outputs.
		</p>
	{/if}
</section>

<style>
	.pending-details {
		width: 100%;
		min-width: 0;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		padding: var(--density-panel, 16px);
		background: var(--surface-solid);
	}
	.pending-details.available,
	:global([data-appearance]) .pending-details.available {
		padding: 0;
		border: 0;
		background: transparent;
		border-radius: 0;
	}
	.available > .coverage {
		margin-block: 0 8px;
		padding: 5px 8px;
		font-size: 11px;
		line-height: 1.5;
	}
	h2 {
		font: var(--weight-display, 750) 24px/1.2 var(--font-display, var(--font-sans));
		margin-top: 5px;
	}
	.detail-scope,
	.coverage,
	.unknown-values,
	.token-empty {
		font-size: 12px;
		line-height: 1.65;
		color: var(--fg-muted);
		margin-block: 10px;
	}
	.coverage {
		border-left: 2px solid var(--accent-ink);
		padding: 8px 10px;
		background: var(--accent-wash);
	}
	.pending-flow {
		display: grid;
		grid-template-columns: minmax(0, 0.75fr) minmax(0, 1.25fr);
		gap: var(--density-gap, 16px);
		margin-block: 0 var(--density-gap, 16px);
		align-items: start;
	}
	.pending-inputs,
	.pending-outputs {
		min-width: 0;
	}
	.pending-inputs {
		grid-column: 1;
		grid-row: 1;
	}
	.pending-outputs {
		grid-column: 2;
		grid-row: 1;
	}
	.pending-flow h2 {
		font-size: 18px;
		line-height: 1.6;
		border-bottom: 1px solid var(--hairline);
		padding-bottom: 4px;
		margin: 0;
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0 8px;
	}
	.pending-flow h2 > span {
		display: block;
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	ol,
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.pending-inputs li {
		padding-block: 8px;
		border-bottom: 1px solid var(--hairline);
	}
	.pending-inputs li > span {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
		margin-top: 4px;
	}
	a {
		color: var(--accent-ink);
		font: 11px/1.65 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
	}
	.outputs {
		display: grid;
		gap: var(--density-gap, 16px);
		margin-top: 6px;
	}
	article {
		min-width: 0;
		padding: 12px;
		border: 1px solid var(--hairline);
		border-radius: 8px;
	}
	.output-heading {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 5px 12px;
	}
	.output-heading > strong {
		font-size: 12px;
	}
	.output-value {
		font: var(--weight-number, 700) 16px/1.5 var(--font-number, var(--font-sans));
		overflow-wrap: anywhere;
	}
	.output-address {
		font-size: 11px;
		color: var(--fg-muted);
		margin-block: 8px;
	}
	.output-address a {
		display: block;
	}
	.output-id {
		display: block;
		margin-block: 6px;
	}
	.output-tokens li {
		padding-block: 8px;
		border-top: 1px solid var(--hairline);
	}
	.output-tokens strong {
		display: block;
		font: 12px/1.7 var(--font-mono);
		overflow-wrap: anywhere;
	}
	summary {
		min-height: 44px;
		align-content: center;
		font-size: 11px;
		color: var(--accent-ink);
		cursor: pointer;
	}
	dl {
		display: grid;
		gap: 10px;
		margin: 0 0 10px;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
	}
	dd {
		font: 11px/1.7 var(--font-mono);
		margin: 4px 0 0;
		overflow-wrap: anywhere;
		word-break: break-all;
	}
	button {
		min-height: 44px;
		max-width: 100%;
		padding: 8px 12px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--accent-ink);
		font-size: 12px;
		margin-top: 8px;
		cursor: pointer;
	}
	.script-copy :global(button) {
		min-height: 44px;
		min-width: 44px;
	}
	:global([data-appearance='prism']) .pending-details {
		background: linear-gradient(145deg, var(--surface-solid), var(--accent-wash));
		border-radius: 20px;
	}
	:global([data-appearance='prism']) .pending-inputs {
		border-left: 2px solid var(--accent-ink);
		padding-left: 12px;
	}
	:global([data-appearance='prism']) article {
		background: var(--surface-solid);
		border-radius: 12px;
		box-shadow: 0 6px 15px color-mix(in srgb, var(--accent-ink) 7%, transparent);
	}
	:global([data-appearance='atelier']) .pending-details {
		border: 0;
		border-block: 4px double var(--fg);
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .pending-flow {
		grid-template-columns: minmax(0, 0.45fr) minmax(0, 1fr);
		gap: 24px;
	}
	:global([data-appearance='atelier']) .pending-inputs {
		padding-right: 16px;
		border-right: 1px solid var(--hairline);
	}
	:global([data-appearance='atelier']) article {
		border: 0;
		border-bottom: 1px solid var(--hairline);
		border-radius: 0;
		padding: 0 0 12px;
	}
	:global([data-appearance='aurora']) .pending-details {
		border-radius: 24px;
	}
	:global([data-appearance='aurora']) .pending-flow {
		grid-template-columns: 1fr;
	}
	:global([data-appearance='aurora']) .pending-inputs {
		grid-column: 1;
		grid-row: 2;
	}
	:global([data-appearance='aurora']) .pending-outputs {
		grid-column: 1;
		grid-row: 1;
	}
	:global([data-appearance='aurora']) .pending-inputs ol,
	:global([data-appearance='aurora']) .outputs {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 12px;
	}
	:global([data-appearance='aurora']) article {
		border-radius: 18px;
		background: var(--accent-wash);
	}
	@media (max-width: 700px) {
		.pending-flow h2 {
			display: flex;
			align-items: baseline;
			flex-wrap: wrap;
			gap: 0 8px;
		}
		:global([data-appearance='atelier']) .pending-details > .coverage {
			margin-block: 6px;
			padding-block: 4px;
		}
		.pending-inputs {
			grid-column: 1;
			grid-row: 2;
		}
		.pending-outputs {
			grid-column: 1;
			grid-row: 1;
		}
		.pending-flow,
		:global([data-appearance='atelier']) .pending-flow,
		:global([data-appearance='aurora']) .pending-inputs ol,
		:global([data-appearance='aurora']) .outputs {
			grid-template-columns: minmax(0, 1fr);
		}
		:global([data-appearance='atelier']) .pending-inputs {
			border-right: 0;
			padding-right: 0;
		}
		a {
			display: block;
			min-height: 44px;
			align-content: center;
		}
	}
</style>
