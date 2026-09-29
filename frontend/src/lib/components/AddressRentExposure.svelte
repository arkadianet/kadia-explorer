<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { SvelteURL } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { formatErg } from '$lib/format/amount';
	import {
		createExposureLoader,
		emptyExposure,
		summarizeExposure,
		rentCategory,
		collectibleNano,
		RENT_CATEGORIES,
		RENT_VIEW_LIMIT,
		type RentCategory
	} from '$lib/rent/exposure';
	let { address }: { address: string } = $props();
	const uid = $props.id();
	let exposure = $state(emptyExposure());
	const loader = createExposureLoader((next) => (exposure = next));
	const horizons = [72, 720, 5040, 20160];
	const horizon = $derived(
		horizons.includes(Number(page.url.searchParams.get('rent_horizon')))
			? Number(page.url.searchParams.get('rent_horizon'))
			: 720
	);
	const category = $derived(page.url.searchParams.get('rent_filter') ?? 'all');
	let offset = $state(0);
	$effect(() => {
		const next = address;
		untrack(() => {
			loader.setAddress(next);
			offset = 0;
		});
	});
	$effect(() => {
		void horizon;
		void category;
		untrack(() => (offset = 0));
	});
	onDestroy(() => loader.stop());
	const result = $derived(exposure.result);
	const summary = $derived(result ? summarizeExposure(result, horizon) : null);
	const filtered = $derived(
		result?.items.filter(
			(box) =>
				!Object.hasOwn(RENT_CATEGORIES, category) ||
				rentCategory(box, result.context.indexed_height, horizon) === category
		) ?? []
	);
	const visible = $derived(filtered.slice(offset, offset + RENT_VIEW_LIMIT));
	function select(key: string, value: string) {
		const url = new SvelteURL(page.url);
		url.searchParams.set(key, value);
		url.hash = 'rent';
		void goto(url.pathname + url.search + url.hash, {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
	}
	async function load() {
		offset = 0;
		await loader.load();
	}
	const erg = (nano: string | bigint) => formatErg(nano.toString(), { maxFrac: 9 });
</script>

<section class="rent-exposure" aria-labelledby={`${uid}-title`} aria-busy={exposure.busy}>
	<header class="rent-head">
		<div>
			<h2 id={`${uid}-title`}>Storage rent exposure</h2>
			{#if result}<div class="snapshot" class:stale={exposure.stale}>
					<strong>{exposure.stale ? 'Previous snapshot · stale' : 'Snapshot at read time'}</strong>
					<span
						>{#if result.context.anchor}<a href={`/blocks/${result.context.anchor.block_id}`}
								>Block {result.context.anchor.height.toLocaleString('en-US')}</a
							>{:else}Canonical block anchor unavailable{/if}</span
					>
					{#if exposure.checkedAt}<time
							datetime={new Date(exposure.checkedAt).toISOString()}
							title={new Date(exposure.checkedAt).toISOString()}
							>Checked {new Date(exposure.checkedAt).toISOString().slice(11, 19)} UTC</time
						>{/if}
				</div>{/if}
		</div>
		<button
			class="load"
			onclick={load}
			disabled={exposure.busy}
			aria-label={exposure.busy
				? 'Checking boxes…'
				: result
					? 'Refresh rent exposure'
					: 'Load rent exposure'}
			>{exposure.busy ? 'Checking…' : result ? 'Refresh' : 'Load exposure'}</button
		>
	</header>
	{#if exposure.error}<p class="warning" role="alert">
			{exposure.error}{#if exposure.stale}
				Previous snapshot retained; it is stale.{/if}
		</p>{/if}
	{#if result && summary}
		{#if !result.context.full_history}<p class="warning">
				Partial chain history{#if result.context.partial_from !== null}
					{` from block ${result.context.partial_from.toLocaleString('en-US')}`}{/if}. Scanned boxes
				only; earlier outputs and pre-index spends may be missing.
			</p>{/if}
		{#if result.truncated}<p class="warning scan-warning">
				Scan limit reached; unscanned boxes may mature sooner. No complete address total is
				available.
			</p>{/if}
		{#if result.context.indexed_height === null}<p class="warning">
				Snapshot height unavailable. Box values are known, but maturity and approaching exposure
				cannot be determined.
			</p>{/if}
		<div class="rent-workspace">
			<aside class="rent-summary" aria-label="Rent exposure summary">
				<p class="eyebrow">
					{summary.complete ? 'Complete indexed unspent set' : 'Scanned boxes only'}
				</p>
				<div class="exposure-total">
					<span>Mature collectible exposure</span><strong
						>{summary.mature === null
							? 'Unavailable'
							: erg(summary.mature)}{#if summary.mature !== null}<small> ERG</small>{/if}</strong
					><span>{summary.counts.mature} mature and collectible boxes</span>
				</div>
				<dl class="primary-totals">
					<div>
						<dt>Within next {horizon.toLocaleString('en-US')} blocks</dt>
						<dd>
							{summary.approaching === null ? 'Unavailable' : `${erg(summary.approaching)} ERG`}
						</dd>
					</div>
				</dl>
			</aside>
			<div class="rent-evidence">
				<div class="filters">
					<div class="filter">
						<label for={`${uid}-horizon`}>Approaching horizon</label><select
							id={`${uid}-horizon`}
							value={horizon}
							onchange={(event) => select('rent_horizon', event.currentTarget.value)}
							>{#each horizons as value (value)}<option {value}
									>{value.toLocaleString('en-US')} blocks</option
								>{/each}</select
						>
					</div>
					<div class="filter">
						<label for={`${uid}-category`}>Box category</label><select
							id={`${uid}-category`}
							value={Object.hasOwn(RENT_CATEGORIES, category) ? category : 'all'}
							onchange={(event) => select('rent_filter', event.currentTarget.value)}
							><option value="all">All scanned boxes</option
							>{#each Object.entries(RENT_CATEGORIES) as [key, label] (key)}<option value={key}
									>{label} ({summary.counts[key as RentCategory]})</option
								>{/each}</select
						>
					</div>
				</div>
				{#if !result.items.length}<p class="empty">
						No unspent boxes were found in this indexed address snapshot.{#if !summary.complete}
							This does not establish zero exposure for the full address.{/if}
					</p>
				{:else if !filtered.length}<p class="empty">No scanned boxes match this category.</p>
				{:else}<ol class="rent-boxes" aria-label="Scanned rent boxes" start={offset + 1}>
						{#each visible as box (box.id)}
							{@const kind = rentCategory(box, result.context.indexed_height, horizon)}
							<li class:mature={kind === 'mature'}>
								<div class="box-title">
									<a class="box-id" href={`/box/${box.id}`}>{box.id}</a><span class="category"
										>{RENT_CATEGORIES[kind]}</span
									>
								</div>
								<div class="box-facts">
									<div>
										<span>Potential collectible charge</span><strong
											>{box.rent.collectible
												? `${erg(collectibleNano(box))} ERG`
												: 'Not collectible'}</strong
										>
									</div>
									<div>
										<span>Maturity height</span><strong
											>{box.rent.maturity_height.toLocaleString('en-US')}</strong
										>
									</div>
									<div>
										<span>At snapshot</span><strong
											>{result.context.indexed_height === null
												? 'Height unavailable'
												: box.rent.maturity_height <= result.context.indexed_height
													? 'Maturity reached'
													: `${(box.rent.maturity_height - result.context.indexed_height).toLocaleString('en-US')} blocks remaining`}</strong
										>
									</div>
								</div>
								<details>
									<summary>Box evidence</summary>
									<dl>
										<div>
											<dt>Box value · nanoERG</dt>
											<dd>{box.value}</dd>
										</div>
										<div>
											<dt>Consensus fee · signed nanoERG</dt>
											<dd>{box.rent.consensus_fee_nano}</dd>
										</div>
										<div>
											<dt>Potential collectible charge · nanoERG</dt>
											<dd>{collectibleNano(box).toString()}</dd>
										</div>
										<div>
											<dt>Declared creation height</dt>
											<dd>{box.creation_height}</dd>
										</div>
										<div>
											<dt>Serialized size</dt>
											<dd>{box.size} bytes</dd>
										</div>
										<div>
											<dt>Token entries</dt>
											<dd>{box.token_count}</dd>
										</div>
									</dl>
								</details>
							</li>{/each}
					</ol>
					<nav class="box-pager" aria-label="Scanned box pages">
						<button
							disabled={offset === 0}
							onclick={() => (offset = Math.max(0, offset - RENT_VIEW_LIMIT))}
							>Previous boxes</button
						><span
							>{offset + 1}–{Math.min(offset + RENT_VIEW_LIMIT, filtered.length)} of {filtered.length}</span
						><button
							disabled={offset + RENT_VIEW_LIMIT >= filtered.length}
							onclick={() => (offset += RENT_VIEW_LIMIT)}>Next boxes</button
						>
					</nav>
				{/if}
			</div>
		</div>
		<details class="scan-totals">
			<summary>Scan totals · {summary.count.toLocaleString('en-US')} boxes</summary>
			<dl>
				<div>
					<dt>Boxes scanned</dt>
					<dd>
						{summary.count.toLocaleString('en-US')} / {result.context.scan_limit.toLocaleString(
							'en-US'
						)} limit
					</dd>
				</div>
				<div>
					<dt>Value held in scanned boxes</dt>
					<dd>{erg(summary.value)} ERG</dd>
				</div>
				<div>
					<dt>Non-collectible boxes</dt>
					<dd>{summary.counts.non_collectible.toLocaleString('en-US')}</dd>
				</div>
			</dl>
		</details>
	{:else if !exposure.busy && !exposure.error}<p class="ready">
			Ready when you are. Opening this tab or a shared link does not start the scan.
		</p>{/if}
	<details class="rent-guide">
		<summary>How rent exposure is calculated</summary>
		<p>
			An explicit check scans up to 5,000 indexed unspent boxes. Exposure uses the positive
			consensus rent fee capped by each box’s value. All maturity distances use the snapshot height,
			not the live tip.
		</p>
		<p>
			Potential rent is an ERG claim against a box, not an automatic debit or a claim about address
			ownership. Spending or recreating a box changes its exposure. Token quantities and any
			resulting token disposition are not included.
		</p>
		<p>
			Earlier outputs and unresolved pre-index spends can leave a partial index’s unspent set
			incomplete. If a scan is truncated, ordering is by maturity among scanned boxes only;
			unscanned boxes may mature sooner.
		</p>
	</details>
</section>

<style>
	.rent-exposure {
		width: 100%;
		min-width: 0;
		padding-block: 8px 0;
	}
	.rent-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.rent-head > div {
		min-width: 0;
	}
	h2 {
		margin: 0;
		font: 600 20px/1.1 var(--font-display, var(--font-sans));
		letter-spacing: -0.035em;
	}
	p {
		margin: 0;
		line-height: 1.45;
	}
	button,
	select {
		color: var(--fg);
		border: 1px solid var(--hairline);
		background: var(--surface);
		border-radius: var(--radius-sm);
		min-height: var(--density-row, 44px);
		padding: 6px 10px;
		font: inherit;
		font-size: 12px;
	}
	button {
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	button:focus-visible,
	select:focus-visible,
	summary:focus-visible {
		outline: 2px solid var(--accent-ink);
		outline-offset: 3px;
	}
	.load {
		background: var(--accent-ink);
		color: var(--bg);
		font-weight: 700;
		flex-shrink: 0;
	}
	.warning {
		padding: 8px 10px;
		margin-block: 8px;
		border-inline-start: 3px solid var(--warn-ink);
		background: var(--surface);
		color: var(--fg);
		font-size: 12px;
	}
	.snapshot {
		display: flex;
		flex-wrap: wrap;
		gap: 3px 14px;
		padding-block: 0;
		margin-top: 6px;
		font-size: 11px;
	}
	.snapshot time {
		color: var(--fg-muted);
	}
	.snapshot.stale {
		color: var(--warn-ink);
	}
	.rent-workspace {
		display: grid;
		gap: var(--density-gap, 16px);
	}
	.rent-summary {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 4px 16px;
		min-width: 0;
		padding-block: 6px;
		border-bottom: 1px solid var(--hairline);
	}
	.eyebrow {
		grid-column: 1/-1;
		font-size: 11px;
		letter-spacing: 0.04em;
		color: var(--accent-ink);
		font-weight: 650;
	}
	.exposure-total {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.exposure-total > span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.exposure-total strong {
		font: 600 23px/1.1 var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
		letter-spacing: -0.045em;
	}
	.exposure-total small {
		font-size: 12px;
		letter-spacing: 0;
		margin-inline-start: 4px;
	}
	dl {
		margin: 0;
		display: grid;
		gap: 8px 16px;
	}
	dl > div {
		min-width: 0;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 2px;
	}
	dd {
		margin: 0;
		font: 500 12px/1.4 var(--font-mono);
		overflow-wrap: anywhere;
	}
	.rent-summary > dl {
		grid-template-columns: 1fr;
		align-content: start;
	}
	.primary-totals dd {
		font-size: 18px;
	}
	.scan-totals {
		border-top: 1px solid var(--hairline);
	}
	.rent-evidence {
		min-width: 0;
	}
	.filters {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.5fr);
		gap: 10px;
		margin-bottom: 8px;
	}
	label {
		font-size: 11px;
		font-weight: 600;
	}
	.filter {
		min-width: 0;
	}
	select {
		display: block;
		width: 100%;
		min-width: 0;
		max-width: 100%;
		margin-top: 3px;
	}
	.rent-boxes {
		list-style: none;
		padding: 0;
		margin: 0;
		border-top: 1px solid var(--hairline);
	}
	.rent-boxes li {
		min-width: 0;
		padding: var(--density-gap, 16px) 0 8px;
		border-bottom: 1px solid var(--hairline);
	}
	.box-title {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 8px;
	}
	.box-id {
		font: 500 12px/1.4 var(--font-mono);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.category {
		font-size: 11px;
		color: var(--fg-muted);
		flex: 0 0 132px;
		text-align: right;
	}
	.mature .category {
		color: var(--warn-ink);
	}
	.box-facts {
		display: grid;
		grid-template-columns: 1.15fr 1fr 1.15fr;
		gap: 8px;
	}
	.box-facts > div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.box-facts span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.box-facts strong {
		font-size: 12px;
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	details {
		margin-top: 6px;
		font-size: 12px;
	}
	summary {
		cursor: pointer;
		color: var(--fg-muted);
		width: fit-content;
		padding-block: 5px;
	}
	details dl {
		padding-block: 8px;
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}
	.box-pager {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin-top: 10px;
	}
	.box-pager span {
		font-size: 11px;
		text-align: center;
	}
	.empty,
	.ready {
		border: 1px dashed var(--hairline);
		padding: 14px;
		margin-top: 12px;
		color: var(--fg-muted);
		font-size: 12px;
	}
	.rent-guide {
		margin-top: 12px;
		border-top: 1px solid var(--hairline);
	}
	.rent-guide p {
		max-width: 90ch;
		margin: 8px 0;
		color: var(--fg-muted);
	}
	:global([data-appearance='prism']) .rent-workspace {
		grid-template-columns: minmax(190px, 0.65fr) minmax(0, 2fr);
	}
	:global([data-appearance='prism']) .rent-summary {
		display: block;
		border: 1px solid var(--hairline);
		padding: var(--density-panel, 16px);
		align-self: start;
		border-radius: 10px;
		background: var(--surface);
	}
	:global([data-appearance='prism']) .rent-summary .eyebrow {
		margin-bottom: 10px;
	}
	:global([data-appearance='prism']) .rent-summary > dl {
		grid-template-columns: 1fr;
		margin-top: 8px;
	}
	:global([data-appearance='atelier']) .rent-workspace {
		grid-template-columns: minmax(0, 1fr) 210px;
	}
	:global([data-appearance='atelier']) .rent-summary {
		display: block;
		grid-column: 2;
		grid-row: 1;
		border-bottom: 0;
		border-inline-start: 3px double var(--hairline);
		padding: 0 0 0 14px;
	}
	:global([data-appearance='atelier']) .rent-evidence {
		grid-column: 1;
		grid-row: 1;
	}
	:global([data-appearance='atelier']) .rent-summary .eyebrow {
		margin-bottom: 8px;
	}
	:global([data-appearance='atelier']) .rent-summary > dl {
		grid-template-columns: 1fr;
		margin-top: 8px;
	}
	:global([data-appearance='atelier']) .rent-head {
		border-bottom: 3px double var(--hairline);
		padding-bottom: 8px;
	}
	:global([data-appearance='atelier']) .rent-boxes li {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: 4px 12px;
	}
	:global([data-appearance='atelier']) .box-title {
		flex-direction: column;
		margin: 0;
		gap: 4px;
	}
	:global([data-appearance='atelier']) .category {
		flex-basis: auto;
		text-align: left;
	}
	:global([data-appearance='atelier']) .rent-boxes details {
		grid-column: 1/-1;
	}
	:global([data-appearance='aurora']) .rent-summary {
		padding: 8px 12px;
		border-block: 1px solid var(--hairline);
		border-inline-start: 3px solid var(--accent-ink);
		background: var(--surface);
	}
	:global([data-appearance='aurora']) .rent-boxes li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr) auto;
		gap: 8px 20px;
		padding: 12px 0;
		align-items: center;
	}
	:global([data-appearance='aurora']) .box-title {
		flex-direction: column;
		gap: 4px;
		margin: 0;
	}
	:global([data-appearance='aurora']) .category {
		flex-basis: auto;
		text-align: left;
	}
	:global([data-appearance='aurora']) .rent-boxes details {
		margin: 0;
	}
	:global([data-appearance='aurora']) .rent-boxes details[open] {
		grid-column: 1/-1;
	}
	@media (min-width: 851px) {
		.filter {
			display: flex;
			align-items: center;
			gap: 8px;
		}
		.filter label {
			flex-shrink: 0;
		}
		.filter select {
			margin-top: 0;
		}
		:global([data-appearance='aurora']) .rent-summary {
			grid-template-columns: minmax(120px, 0.7fr) minmax(0, 1fr) minmax(0, 1fr);
			align-items: center;
		}
		:global([data-appearance='aurora']) .rent-summary .eyebrow {
			grid-column: auto;
		}
	}
	@media (max-width: 850px) {
		:global([data-appearance]) .rent-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		:global([data-appearance]) .rent-summary {
			display: grid;
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
			gap: 4px 12px;
			padding: 6px 0;
			border: 0;
			border-bottom: 1px solid var(--hairline);
			border-radius: 0;
			grid-column: auto;
			grid-row: auto;
			background: transparent;
		}
		:global([data-appearance]) .rent-summary .eyebrow {
			margin: 0;
		}
		:global([data-appearance]) .rent-summary > dl {
			grid-template-columns: 1fr;
			margin: 0;
			gap: 6px 10px;
		}
		:global([data-appearance='atelier']) .rent-evidence {
			grid-column: auto;
			grid-row: auto;
		}
		:global([data-appearance='atelier']) .rent-boxes li,
		:global([data-appearance='aurora']) .rent-boxes li {
			display: block;
			padding: 12px 0 4px;
		}
		.rent-head {
			gap: 8px;
		}
		h2 {
			font-size: 19px;
		}
		.exposure-total strong {
			font-size: 24px;
		}
		.box-title {
			flex-direction: column;
			gap: 4px;
			margin-bottom: 8px;
		}
		.category {
			flex-basis: auto;
			text-align: left;
		}
		:global([data-appearance='atelier']) .box-title,
		:global([data-appearance='aurora']) .box-title {
			margin-bottom: 8px;
		}
		button,
		select,
		summary {
			min-height: 44px;
		}
		summary {
			display: flex;
			align-items: center;
			gap: 6px;
		}
		summary::before {
			content: '▸';
		}
		details[open] > summary::before {
			content: '▾';
		}
		details dl {
			grid-template-columns: 1fr 1fr;
		}
	}
	@media (max-width: 360px) {
		.rent-head {
			align-items: stretch;
			flex-direction: column;
		}
		.load {
			width: 100%;
		}
		.exposure-total strong {
			font-size: 22px;
		}
		.box-pager {
			flex-wrap: wrap;
		}
		.box-pager span {
			width: 100%;
			order: -1;
		}
		.box-pager button {
			flex: 1;
		}
	}
</style>
