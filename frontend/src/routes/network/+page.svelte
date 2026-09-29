<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { status } from '$lib/status/status.svelte';
	import { formatErg } from '$lib/format/amount';
	import {
		chartHeights,
		createHistoryLoader,
		historyQuery,
		range,
		type HistoryState,
		type NetworkMetric
	} from '$lib/network/history';
	let from = $state('');
	let to = $state('');
	let buckets = $state('60');
	let pin = $state<string | undefined>();
	let localError = $state<string | null>(null);
	let historyState = $state<HistoryState>({ loading: false, data: null, error: null });
	let metric = $state<NetworkMetric>('transaction_count');
	const metrics: { key: NetworkMetric; title: string; unit: string }[] = [
		{ key: 'transaction_count', title: 'Transactions', unit: 'transactions' },
		{ key: 'fees', title: 'Transaction fees', unit: 'nanoERG' },
		{ key: 'size_bytes', title: 'Block size', unit: 'bytes' },
		{ key: 'difficulty_end', title: 'End-block difficulty', unit: 'difficulty' }
	];
	const loader = createHistoryLoader((next) => {
		historyState = next;
		if (next.data) pin = next.data.anchor.block_id;
	});
	const selected = $derived(metrics.find((item) => item.key === metric)!);
	const heights = $derived(
		historyState.data ? chartHeights(historyState.data.buckets, metric) : []
	);
	const share = $derived(
		historyState.data
			? '/network?' +
					historyQuery({
						from_height: historyState.data.totals.from_height,
						to_height: historyState.data.totals.to_height,
						buckets: historyState.data.requested_buckets,
						end_block_id: historyState.data.anchor.block_id
					})
			: null
	);
	$effect(() => {
		const query = page.url.searchParams;
		loader.clear();
		localError = null;
		from = query.get('from_height') ?? '';
		to = query.get('to_height') ?? '';
		buckets = query.get('buckets') ?? '60';
		pin = query.get('end_block_id') ?? undefined;
	});
	onDestroy(() => loader.stop());
	function changed() {
		loader.clear();
		pin = undefined;
		localError = null;
	}
	function recent() {
		const tip = status.current?.indexed;
		if (tip == null || tip < 1) return;
		changed();
		to = String(tip);
		from = String(Math.max(1, tip - 719));
	}
	function load(event: SubmitEvent) {
		event.preventDefault();
		localError = null;
		try {
			void loader.load(range(from.trim(), to.trim(), buckets.trim(), pin));
		} catch (reason) {
			loader.clear();
			localError = reason instanceof Error ? reason.message : 'Check the selected range.';
		}
	}
	const exact = (value: string) => BigInt(value).toLocaleString('en-US');
	function time(value: number) {
		const date = new Date(value);
		return Number.isNaN(date.valueOf())
			? value + ' ms since epoch'
			: date.toISOString().replace('T', ' ').replace('.000Z', ' UTC');
	}
</script>

<svelte:head
	><title>Network history — Kadia Explorer</title><meta
		name="description"
		content="Inspect exact transaction, fee, block-size and difficulty history across a bounded range of indexed Ergo blocks."
	/></svelte:head
>
<div class="network-history">
	<header class="network-intro">
		<p class="eyebrow">NETWORK / CANONICAL BLOCK HISTORY</p>
		<h1>Network history</h1>
		<p>Activity, fees, size and difficulty.</p>
	</header>
	<form onsubmit={load} aria-label="Network history range">
		<div class="range-fields">
			<label
				>From height<input
					inputmode="numeric"
					maxlength="10"
					bind:value={from}
					oninput={changed}
					required
				/></label
			><label
				>To height<input
					inputmode="numeric"
					maxlength="10"
					bind:value={to}
					oninput={changed}
					required
				/></label
			><label
				>Buckets<input
					aria-label="Maximum buckets"
					inputmode="numeric"
					maxlength="3"
					bind:value={buckets}
					oninput={changed}
					required
				/></label
			>
		</div>
		<div class="range-actions">
			<button type="submit" disabled={historyState.loading}
				>{historyState.loading ? 'Loading range…' : 'Load network history'}</button
			><button
				type="button"
				class="secondary"
				onclick={recent}
				disabled={!status.current?.indexed}
				aria-label="Use latest 720 blocks">Latest 720 blocks</button
			>
		</div>
		{#if pin}<p class="pin">
				<span
					>Pinned end: <a href={'/blocks/' + pin} title={pin} aria-label={'Pinned end block ' + pin}
						>{pin.slice(0, 8)}…{pin.slice(-8)}</a
					></span
				>
				<button type="button" class="secondary" onclick={changed} aria-label="Clear end-block pin"
					>Clear pin</button
				>
			</p>{/if}
	</form>
	{#if localError || historyState.error}<div class="notice" role="alert">
			<h2>Range unavailable</h2>
			<p>{localError ?? historyState.error}</p>
			<p>No partial chart or earlier result is shown.</p>
		</div>{/if}
	{#if historyState.data}
		{@const data = historyState.data}
		<section class="network-results" aria-label="Network history results">
			<div class="range-heading">
				<div>
					<h2>
						{data.totals.from_height.toLocaleString('en-US')} → {data.totals.to_height.toLocaleString(
							'en-US'
						)}
					</h2>
				</div>
				<div class="result-links">
					<a href={share!} aria-label="Open pinned range">Pinned link ↗</a>
					<a
						href={`/mining?from_height=${data.totals.from_height}&to_height=${data.totals.to_height}&end_block_id=${data.anchor.block_id}`}
						aria-label="Inspect mining signals">Mining ↗</a
					>
				</div>
			</div>
			<p class="coverage-strip">
				{data.full_history
					? 'Full retained history.'
					: 'The index is partial' +
						(data.partial_from === null
							? '.'
							: ', from height ' + data.partial_from.toLocaleString('en-US') + '.')}
				All {data.totals.block_count.toLocaleString('en-US')} requested headers present.
			</p>
			<div class="summary">
				<div>
					<span>Blocks</span><strong>{data.totals.block_count.toLocaleString('en-US')}</strong>
				</div>
				<div><span>Transactions</span><strong>{exact(data.totals.transaction_count)}</strong></div>
				<div>
					<span>Transaction fees</span><strong title={data.totals.fees + ' nanoERG'}
						>{formatErg(data.totals.fees)} <small>ERG</small></strong
					>
				</div>
				<div><span>Block bytes</span><strong>{exact(data.totals.size_bytes)}</strong></div>
			</div>
			<div class="visual">
				<div class="chart-controls">
					<label
						>Chart metric<select bind:value={metric}
							>{#each metrics as item (item.key)}<option value={item.key}>{item.title}</option
								>{/each}</select
						></label
					>
					<p>{data.buckets.length} buckets · up to {data.bucket_width} blocks each</p>
				</div>
				<figure>
					<svg
						viewBox="0 0 1000 180"
						role="img"
						aria-label={selected.title + ' by block-height bucket'}
						preserveAspectRatio="none"
						><line
							x1="0"
							x2="1000"
							y1="165"
							y2="165"
							class="axis"
						/>{#each data.buckets as bucket, index (bucket.from_height)}<rect
								x={(index * 1000) / data.buckets.length + 1}
								y={165 - heights[index] * 1.5}
								width={Math.max(1, 1000 / data.buckets.length - 2)}
								height={heights[index] * 1.5}
								><title
									>Blocks {bucket.from_height}–{bucket.to_height}: {bucket[metric]}
									{selected.unit}</title
								></rect
							>{/each}</svg
					>
					<figcaption>
						<span>Block {data.totals.from_height}</span><span>Block {data.totals.to_height}</span>
					</figcaption>
				</figure>
			</div>

			<details class="raw-values" open>
				<summary>Exact bucket values</summary>
				<!-- svelte-ignore a11y_no_noninteractive_tabindex (The bounded table region must support keyboard scrolling.) -->
				<div
					class="table-scroll"
					role="region"
					aria-label="Exact network history values"
					tabindex="0"
				>
					<table>
						<thead
							><tr
								><th>Block range</th><th>Blocks</th><th>Transactions</th><th>Fees (nanoERG)</th><th
									>Bytes</th
								><th>Difficulty end</th><th>Difficulty min / max</th><th>Header time span (UTC)</th
								></tr
							></thead
						><tbody
							>{#each data.buckets as bucket (bucket.from_height)}<tr
									><th>{bucket.from_height}–{bucket.to_height}</th><td>{bucket.block_count}</td><td
										>{bucket.transaction_count}</td
									><td>{bucket.fees}</td><td>{bucket.size_bytes}</td><td>{bucket.difficulty_end}</td
									><td>{bucket.difficulty_min}<br />{bucket.difficulty_max}</td><td
										>{time(bucket.earliest_timestamp)}<br />{time(bucket.latest_timestamp)}</td
									></tr
								>{/each}</tbody
						>
					</table>
				</div>
			</details>
			<details class="coverage">
				<summary>Methodology and snapshot evidence</summary>
				<p>
					Up to 20,160 blocks and 120 height buckets. Loading is explicit, with no background
					refresh. Refresh preserves the end-block pin and rejects a changed canonical block until
					you clear it.
				</p>
				<p>
					Bars start at zero and scale to this range’s maximum. Difficulty is the last block’s exact
					value in each bucket; the table also shows its minimum and maximum. Height spacing does
					not imply equal elapsed time. 720 blocks is a count, not a guaranteed day.
				</p>
				<p>
					One snapshot of canonical indexed block headers, observed at indexed height {data.indexed_height.toLocaleString(
						'en-US'
					)}. End anchor: <a href={'/blocks/' + data.anchor.block_id}>{data.anchor.block_id}</a>.
				</p>
				<p>Coverage describes only the requested range, not activity outside it.</p>
				<p>
					Header timestamps span {time(data.totals.earliest_timestamp)} to {time(
						data.totals.latest_timestamp
					)}. Timestamps can move backwards; block order is canonical.
				</p>
				<p>
					Fees come from indexed block totals. Difficulty is not a measured hashrate. No circulating
					supply, prices, miner identity or ownership is inferred.
				</p>
			</details>
		</section>
	{:else if !historyState.loading && !localError && !historyState.error}<section class="empty">
			<h2>Choose the blocks to compare.</h2>
			<p>
				Set an inclusive height range or use the latest indexed 720 blocks. Shareable pinned links
				keep the same end-block identity and ask the viewer to load explicitly.
			</p>
		</section>{/if}
</div>

<style>
	.network-history {
		width: 100%;
		max-width: 1440px;
		margin-inline: auto;
		min-width: 0;
	}
	.network-intro {
		margin-bottom: var(--density-gap, 16px);
	}
	.eyebrow {
		font: 10px var(--font-mono);
		letter-spacing: 0.08em;
		color: var(--accent-ink);
		margin: 0 0 4px;
	}
	h1 {
		font-size: var(--density-title, 32px);
		line-height: 1.15;
		letter-spacing: -0.035em;
		margin: 0 0 6px;
	}
	h2 {
		font-size: 18px;
		line-height: 1.3;
		margin: 0;
		overflow-wrap: anywhere;
	}
	p {
		font-size: 12px;
		line-height: 1.6;
		color: var(--fg-muted);
		margin: 0;
	}
	form,
	.empty,
	.notice,
	.network-results {
		border: 1px solid var(--hairline);
		background: var(--surface-solid);
		border-radius: var(--radius-card);
		padding: var(--density-panel, 16px);
		margin-block: var(--density-gap, 16px);
		min-width: 0;
	}
	form {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 8px var(--density-gap, 16px);
		align-items: end;
	}
	.range-fields {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(64px, 0.5fr);
		gap: 8px;
	}
	label {
		display: grid;
		gap: 4px;
		font-size: 11px;
		color: var(--fg-muted);
		min-width: 0;
	}
	input,
	select {
		width: 100%;
		min-width: 0;
		min-height: var(--density-row, 44px);
		color: var(--fg);
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		padding: 7px 8px;
		font: 13px var(--font-mono);
	}
	.range-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	button {
		min-height: var(--density-row, 44px);
		border: 1px solid var(--btn-fill);
		color: var(--btn-fill-fg);
		background: var(--btn-fill);
		padding: 7px 10px;
		border-radius: var(--radius-control);
		font-size: 12px;
		cursor: pointer;
	}
	button.secondary {
		background: transparent;
		color: var(--fg);
		border-color: var(--hairline);
	}
	button:disabled {
		opacity: 0.55;
		cursor: default;
	}
	a {
		color: var(--accent-ink);
		text-decoration: underline;
		text-underline-offset: 3px;
		overflow-wrap: anywhere;
	}
	.pin {
		grid-column: 1/-1;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		border-top: 1px solid var(--hairline);
		padding-top: 8px;
	}
	.pin button {
		min-height: 32px;
		padding-block: 4px;
	}
	.network-results {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.range-heading {
		display: flex;
		flex-wrap: wrap;
		gap: 4px var(--density-gap, 16px);
		justify-content: space-between;
		align-items: center;
	}
	.result-links {
		display: flex;
		gap: 12px;
		font-size: 11px;
	}
	.coverage-strip {
		font-size: 11px;
	}
	.summary {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--density-gap, 16px);
		padding-block: 8px;
		border-block: 1px solid var(--hairline);
	}
	.summary > div {
		display: grid;
		gap: 3px;
		min-width: 0;
	}
	.summary span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.summary strong {
		font-size: 23px;
		line-height: 1.3;
		font-weight: 600;
		letter-spacing: -0.025em;
		overflow-wrap: anywhere;
	}
	small {
		font-size: 11px;
		font-weight: 400;
	}
	.chart-controls {
		display: flex;
		gap: 12px;
		align-items: end;
		justify-content: space-between;
	}
	.chart-controls label {
		grid-template-columns: auto minmax(140px, 220px);
		align-items: center;
		gap: 8px;
	}
	.chart-controls p {
		font-size: 11px;
	}
	figure {
		margin: 8px 0 0;
	}
	svg {
		display: block;
		width: 100%;
		height: 140px;
		overflow: visible;
	}
	rect {
		fill: var(--accent-ink);
	}
	.axis {
		stroke: var(--hairline);
	}
	figcaption {
		display: flex;
		justify-content: space-between;
		color: var(--fg-muted);
		font: 10px var(--font-mono);
	}
	.coverage {
		border-top: 1px solid var(--hairline);
		min-width: 0;
	}
	.coverage p {
		margin-block: 8px;
	}
	.raw-values {
		min-width: 0;
	}
	summary {
		cursor: pointer;
		font-size: 12px;
		padding-block: 10px;
	}
	.table-scroll {
		overflow: auto;
		max-height: 440px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
	}
	table {
		border-collapse: collapse;
		width: 100%;
		font: 12px var(--font-mono);
	}
	th,
	td {
		text-align: left;
		padding: 8px;
		height: var(--density-row, 44px);
		white-space: nowrap;
		border-bottom: 1px solid var(--hairline);
	}
	thead {
		background: var(--surface-solid);
		position: sticky;
		top: 0;
	}
	:global([data-appearance='prism']) .network-results {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(210px, 0.35fr);
		gap: 10px var(--density-gap, 16px);
		border-radius: 18px;
		box-shadow: 0 12px 32px color-mix(in srgb, var(--accent-ink) 10%, transparent);
	}
	:global([data-appearance='prism']) .range-heading,
	:global([data-appearance='prism']) .coverage-strip,
	:global([data-appearance='prism']) .raw-values,
	:global([data-appearance='prism']) .coverage {
		grid-column: 1/-1;
	}
	:global([data-appearance='prism']) .visual {
		grid-column: 1;
		grid-row: 3;
	}
	:global([data-appearance='prism']) .summary {
		grid-column: 2;
		grid-row: 3;
		grid-template-columns: 1fr 1fr;
		align-content: start;
		border: 1px solid var(--hairline);
		border-radius: 12px;
		padding: var(--density-panel, 16px);
		background: var(--accent-wash);
	}
	:global([data-appearance='atelier']) .network-results {
		border: 0;
		border-top: 3px double var(--fg);
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
		display: grid;
		grid-template-columns: minmax(140px, 0.25fr) minmax(0, 1fr);
		gap: 10px var(--density-gap, 16px);
	}
	:global([data-appearance='atelier']) .range-heading,
	:global([data-appearance='atelier']) .coverage-strip,
	:global([data-appearance='atelier']) .raw-values,
	:global([data-appearance='atelier']) .coverage {
		grid-column: 1/-1;
	}
	:global([data-appearance='atelier']) .summary {
		grid-column: 1;
		grid-row: 3;
		grid-template-columns: 1fr;
		align-content: start;
		border: 0;
		padding: 0;
		gap: 8px;
	}
	:global([data-appearance='atelier']) .summary > div {
		gap: 0;
	}
	:global([data-appearance='atelier']) .summary strong {
		font-size: 18px;
	}
	:global([data-appearance='atelier']) .visual {
		grid-column: 2;
		grid-row: 3;
		border-left: 1px solid var(--hairline);
		padding-left: var(--density-panel, 16px);
	}
	:global([data-appearance='atelier']) form {
		border-inline: 0;
		background: transparent;
		border-radius: 0;
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .network-intro {
		text-align: center;
	}
	:global([data-appearance='aurora']) form {
		border-radius: 18px;
		box-shadow: 0 12px 32px color-mix(in srgb, var(--accent-ink) 8%, transparent);
	}
	:global([data-appearance='aurora']) .network-results {
		border: 0;
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .visual {
		padding: var(--density-panel, 16px);
		border: 1px solid var(--hairline);
		border-radius: 22px;
		background: var(--surface-solid);
		margin-inline: 12px;
	}
	:global([data-appearance='aurora']) .summary {
		text-align: center;
		border: 0;
	}
	@media (max-width: 900px) {
		.network-results {
			gap: 6px;
		}
		form {
			grid-template-columns: 1fr;
		}
		.range-actions {
			display: grid;
			grid-template-columns: 1fr 1fr;
		}
		input,
		select,
		button {
			min-height: 44px;
		}
		.pin button {
			min-height: 44px;
		}
		.pin a,
		.result-links a {
			display: inline-flex;
			align-items: center;
			min-height: 44px;
		}
		summary {
			min-height: 44px;
			padding-block: 12px;
		}
		.summary {
			grid-template-columns: 1fr 1fr;
		}
		.summary strong {
			font-size: 19px;
		}
		.range-heading {
			gap: 0 12px;
		}
		.chart-controls label {
			width: 60%;
			grid-template-columns: 1fr;
			gap: 4px;
		}
		.chart-controls p {
			max-width: 35%;
		}
		svg {
			height: 100px;
		}
		:global([data-appearance='prism']) .network-results,
		:global([data-appearance='atelier']) .network-results {
			display: flex;
			gap: 6px;
		}
		:global([data-appearance='prism']) .summary,
		:global([data-appearance='atelier']) .summary {
			grid-template-columns: 1fr 1fr;
			padding: 8px;
		}
		:global([data-appearance='atelier']) .visual {
			border-left: 0;
			padding-left: 0;
		}
		:global([data-appearance='aurora']) .visual {
			margin-inline: 0;
			padding: 8px;
		}
	}
	@media (max-width: 560px) {
		.range-fields {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(58px, 0.6fr);
		}
		.pin {
			flex-wrap: wrap;
		}
	}
</style>
