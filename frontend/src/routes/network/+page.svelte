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
		<h1>The chain, over time.</h1>
		<p>
			Compare activity, fees, block size and difficulty across an exact height range. Every chart
			has the raw values behind it.
		</p>
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
				>Maximum buckets<input
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
			><button type="button" class="secondary" onclick={recent} disabled={!status.current?.indexed}
				>Use latest 720 blocks</button
			>{#if pin}<button type="button" class="secondary" onclick={changed}
					>Clear end-block pin</button
				>{/if}
		</div>
		<p>
			Up to 20,160 blocks and 120 height buckets. 720 blocks is a count, not a guaranteed day.
			Loading is explicit; ranges do not refresh in the background.
		</p>
		{#if pin}<p class="pin">
				Pinned end block: <a href={'/blocks/' + pin}>{pin}</a>. A changed block rejects refresh
				until you clear this pin.
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
					<p class="eyebrow">COMPLETE SELECTED RANGE</p>
					<h2>
						{data.totals.from_height.toLocaleString('en-US')} → {data.totals.to_height.toLocaleString(
							'en-US'
						)}
					</h2>
				</div>
				<a href={share!}>Open pinned range ↗</a>
				<a
					href={`/mining?from_height=${data.totals.from_height}&to_height=${data.totals.to_height}&end_block_id=${data.anchor.block_id}`}
					>Inspect mining signals ↗</a
				>
			</div>
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
				<p class="chart-note">
					Bars start at zero and scale to this range’s maximum. Difficulty is the last block’s exact
					value in each bucket; the table also shows its minimum and maximum. Height spacing does
					not imply equal elapsed time.
				</p>
			</div>
			<aside class="coverage">
				<h3>What this range covers</h3>
				<p>
					One snapshot of canonical indexed block headers, observed at indexed height {data.indexed_height.toLocaleString(
						'en-US'
					)}. End anchor: <a href={'/blocks/' + data.anchor.block_id}>{data.anchor.block_id}</a>.
				</p>
				<p>
					{data.full_history
						? 'The index reports full retained history.'
						: 'The index is partial' +
							(data.partial_from === null
								? '.'
								: ', from height ' + data.partial_from.toLocaleString('en-US') + '.')} All requested headers
					are present; this does not describe activity outside the selected range.
				</p>
				<p>
					Header timestamps span {time(data.totals.earliest_timestamp)} to {time(
						data.totals.latest_timestamp
					)}. Timestamps can move backwards; block order is canonical.
				</p>
				<p>
					Fees come from indexed block totals. Difficulty is not a measured hashrate. No circulating
					supply, prices, miner identity or ownership is inferred.
				</p>
			</aside>
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
		max-width: 72ch;
		margin-bottom: 28px;
	}
	.eyebrow {
		font: 10px var(--font-mono);
		letter-spacing: 0.1em;
		color: var(--accent-ink);
		margin-bottom: 12px;
	}
	h1 {
		font-size: clamp(34px, 5vw, 65px);
		line-height: 1.05;
		letter-spacing: -0.05em;
		margin-bottom: 16px;
	}
	p {
		font-size: 12px;
		line-height: 1.8;
		color: var(--fg-muted);
	}
	form,
	.empty,
	.notice,
	.network-results {
		border: 1px solid var(--hairline);
		background: var(--surface-solid);
		border-radius: var(--radius-card);
		padding: 24px;
		margin-block: 22px;
		min-width: 0;
	}
	.range-fields {
		display: grid;
		grid-template-columns: 1fr 1fr 0.7fr;
		gap: 16px;
	}
	label {
		display: grid;
		gap: 8px;
		font-size: 11px;
		color: var(--fg-muted);
		min-width: 0;
	}
	input,
	select {
		width: 100%;
		min-width: 0;
		color: var(--fg);
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		padding: 11px;
		font: 13px var(--font-mono);
	}
	.range-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-block: 18px 12px;
	}
	button {
		border: 1px solid var(--btn-fill);
		color: var(--btn-fill-fg);
		background: var(--btn-fill);
		padding: 11px 15px;
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
		overflow-wrap: anywhere;
	}
	.range-heading {
		display: flex;
		gap: 16px;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 28px;
	}
	h2 {
		font-size: clamp(20px, 3vw, 30px);
		letter-spacing: -0.025em;
		overflow-wrap: anywhere;
	}
	.range-heading > a {
		font-size: 11px;
	}
	.summary {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 22px;
		margin-bottom: 32px;
	}
	.summary > div {
		display: grid;
		gap: 12px;
		min-width: 0;
	}
	.summary span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.summary strong {
		font-family: var(--font-number, var(--font-sans));
		font-size: clamp(20px, 3vw, 32px);
		font-weight: var(--weight-number, 600);
		overflow-wrap: anywhere;
	}
	small {
		font-size: 12px;
		font-weight: 400;
	}
	.chart-controls {
		display: flex;
		gap: 20px;
		align-items: end;
		justify-content: space-between;
	}
	.chart-controls label {
		width: 240px;
	}
	figure {
		margin: 22px 0 12px;
	}
	svg {
		display: block;
		width: 100%;
		height: 180px;
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
	.chart-note {
		font-size: 11px;
		max-width: 95ch;
	}
	.coverage {
		margin-top: 30px;
		border-top: 1px solid var(--hairline);
		padding-top: 20px;
	}
	h3 {
		font-size: 14px;
		margin-bottom: 10px;
	}
	.coverage p {
		margin-block: 10px;
	}
	.raw-values {
		margin-top: 28px;
		min-width: 0;
	}
	summary {
		cursor: pointer;
		font-size: 13px;
		padding-block: 12px;
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
		font: 11px var(--font-mono);
	}
	th,
	td {
		text-align: left;
		padding: 12px;
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
		grid-template-columns: minmax(0, 1fr) minmax(220px, 0.36fr);
		gap: 26px;
		border-radius: 24px;
		box-shadow: 0 20px 48px color-mix(in srgb, var(--accent-ink) 12%, transparent);
	}
	:global([data-appearance='prism']) .range-heading,
	:global([data-appearance='prism']) .summary,
	:global([data-appearance='prism']) .raw-values {
		grid-column: 1/-1;
		margin: 0;
	}
	:global([data-appearance='prism']) .coverage {
		margin: 0;
		border: 1px solid var(--hairline);
		border-radius: 16px;
		padding: 18px;
		background: var(--accent-wash);
	}
	:global([data-appearance='atelier']) .network-results {
		border: 0;
		border-top: 3px double var(--fg);
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
		display: grid;
		grid-template-columns: minmax(150px, 0.3fr) minmax(0, 1fr);
		gap: 30px;
	}
	:global([data-appearance='atelier']) .range-heading,
	:global([data-appearance='atelier']) .raw-values {
		grid-column: 1/-1;
		margin: 0;
	}
	:global([data-appearance='atelier']) .summary {
		grid-column: 1;
		grid-row: 2;
		grid-template-columns: 1fr;
		align-content: start;
	}
	:global([data-appearance='atelier']) .visual {
		grid-column: 2;
		grid-row: 2;
	}
	:global([data-appearance='atelier']) .coverage {
		grid-column: 1/-1;
	}
	:global([data-appearance='atelier']) form {
		border: 0;
		border-top: 1px solid var(--hairline);
		border-bottom: 1px solid var(--hairline);
		background: transparent;
		border-radius: 0;
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .network-intro {
		text-align: center;
		margin: 30px auto 42px;
	}
	:global([data-appearance='aurora']) form {
		max-width: 1000px;
		margin-inline: auto;
		border-radius: 28px;
		box-shadow: 0 18px 50px color-mix(in srgb, var(--accent-ink) 8%, transparent);
	}
	:global([data-appearance='aurora']) .network-results {
		border: 0;
		border-radius: 0;
		background: transparent;
		padding: 22px 0;
	}
	:global([data-appearance='aurora']) .visual {
		padding: 30px;
		border: 1px solid var(--hairline);
		border-radius: 30px;
		background: var(--surface-solid);
	}
	:global([data-appearance='aurora']) .summary {
		text-align: center;
		padding-block: 20px;
	}
	@media (max-width: 800px) {
		form,
		.network-results,
		.notice,
		.empty {
			padding: 18px;
		}
		.range-fields {
			grid-template-columns: 1fr 1fr;
		}
		.range-fields label:last-child {
			grid-column: 1/-1;
		}
		.summary {
			grid-template-columns: 1fr 1fr;
		}
		.range-heading,
		.chart-controls {
			align-items: start;
			flex-direction: column;
		}
		.chart-controls label {
			width: 100%;
		}
		:global([data-appearance='prism']) .network-results,
		:global([data-appearance='atelier']) .network-results {
			display: block;
		}
		:global([data-appearance='prism']) .summary,
		:global([data-appearance='atelier']) .summary {
			display: grid;
			grid-template-columns: 1fr 1fr;
			margin-block: 24px;
		}
		:global([data-appearance='prism']) .coverage,
		:global([data-appearance='atelier']) .coverage {
			margin-top: 22px;
		}
		:global([data-appearance='aurora']) .visual {
			padding: 18px;
		}
	}
</style>
