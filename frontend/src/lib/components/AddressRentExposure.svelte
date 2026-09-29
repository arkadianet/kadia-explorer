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
			<p class="eyebrow">Box longevity · anchored evidence</p>
			<h2 id={`${uid}-title`}>Storage rent exposure</h2>
			<p>
				See which indexed unspent boxes have reached rent maturity, and which are approaching it.
			</p>
		</div>
		<button class="load" onclick={load} disabled={exposure.busy}
			>{exposure.busy
				? 'Checking boxes…'
				: result
					? 'Refresh rent exposure'
					: 'Load rent exposure'}</button
		>
	</header>
	<p class="explanation">
		An explicit check scans up to 5,000 indexed unspent boxes. Potential rent is an ERG claim
		against an individual box, not an automatic debit or a claim about address ownership. Spending
		or recreating a box changes its exposure.
	</p>
	{#if exposure.error}<p class="warning" role="alert">
			{exposure.error}{#if exposure.stale}
				Previous snapshot retained; it is stale.{/if}
		</p>{/if}
	{#if result && summary}
		<div class="snapshot" class:stale={exposure.stale}>
			<strong>{exposure.stale ? 'Previous snapshot · stale' : 'Snapshot at read time'}</strong>
			<span
				>{#if result.context.anchor}<a href={`/blocks/${result.context.anchor.block_id}`}
						>Block {result.context.anchor.height.toLocaleString('en-US')}</a
					>{:else}Canonical block anchor unavailable{/if}</span
			>
			{#if exposure.checkedAt}<time datetime={new Date(exposure.checkedAt).toISOString()}
					>Checked {new Date(exposure.checkedAt)
						.toISOString()
						.replace('T', ' ')
						.replace(/\.\d{3}Z$/, ' UTC')}</time
				>{/if}
		</div>
		{#if !result.context.full_history}<p class="warning">
				Partial chain history{#if result.context.partial_from !== null}
					{` from block ${result.context.partial_from.toLocaleString('en-US')}`}{/if}. Earlier
				outputs and unresolved pre-index spends can leave this address’s unspent set incomplete.
				These amounts cover scanned boxes only.
			</p>{/if}
		{#if result.truncated}<p class="warning scan-warning">
				Scan limit reached. This is a partial sample sorted by maturity among the boxes scanned;
				unscanned boxes may mature sooner. No complete address total is available.
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
				<dl>
					<div>
						<dt>Within next {horizon.toLocaleString('en-US')} blocks</dt>
						<dd>
							{summary.approaching === null ? 'Unavailable' : `${erg(summary.approaching)} ERG`}
						</dd>
					</div>
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
				<p class="explanation">
					Exposure uses the positive consensus rent fee capped by each box’s value. Token quantities
					and any resulting token disposition are not included. All block distances use this
					snapshot’s height, not the live tip.
				</p>
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
	{:else if !exposure.busy && !exposure.error}<p class="ready">
			Ready when you are. Opening this tab or a shared link does not start the scan.
		</p>{/if}
</section>

<style>
	.rent-exposure {
		width: 100%;
		min-width: 0;
		padding-block: var(--space-5);
	}
	.rent-head {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 24px;
	}
	.rent-head > div {
		max-width: 65ch;
		min-width: 0;
	}
	.eyebrow {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.12em;
		color: var(--accent-ink);
		font-weight: 700;
		margin: 0 0 8px;
	}
	h2 {
		margin: 0 0 10px;
		font-family: var(--font-display, var(--font-sans));
		font-size: clamp(24px, 3vw, 38px);
		line-height: 1.05;
		letter-spacing: -0.035em;
	}
	p {
		margin: 0 0 16px;
		line-height: 1.6;
	}
	.explanation {
		font-size: 12px;
		color: var(--fg-muted);
		max-width: 100ch;
		margin-top: 16px;
	}
	button,
	select {
		color: var(--fg);
		border: 1px solid var(--hairline);
		background: var(--surface);
		border-radius: var(--radius-sm);
		min-height: 40px;
		padding: 8px 12px;
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
		padding: 14px 16px;
		border-inline-start: 3px solid var(--warn-ink);
		background: var(--surface);
		color: var(--fg);
		font-size: 13px;
	}
	.snapshot {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 20px;
		padding: 14px 0;
		margin: 12px 0;
		border-block: 1px solid var(--hairline);
		font-size: 12px;
	}
	.snapshot time {
		color: var(--fg-muted);
	}
	.snapshot.stale {
		border-color: var(--warn-ink);
	}
	.rent-workspace {
		display: grid;
		gap: 24px;
	}
	.rent-summary {
		min-width: 0;
	}
	.exposure-total {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 20px;
	}
	.exposure-total > span {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.exposure-total strong {
		font: 600 clamp(27px, 4vw, 44px)/1.1 var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
		letter-spacing: -0.05em;
	}
	.exposure-total small {
		font-size: 14px;
		letter-spacing: 0;
	}
	dl {
		margin: 0;
		display: grid;
		gap: 12px;
	}
	dl > div {
		min-width: 0;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 3px;
	}
	dd {
		margin: 0;
		font: 500 13px/1.5 var(--font-mono);
		overflow-wrap: anywhere;
	}
	.rent-summary > dl {
		grid-template-columns: repeat(4, minmax(0, 1fr));
		padding-block: 16px;
		border-block: 1px solid var(--hairline);
	}
	.rent-evidence {
		min-width: 0;
	}
	.filters {
		display: grid;
		grid-template-columns: 1fr 1.4fr;
		gap: 12px;
		margin-bottom: 16px;
	}
	label {
		font-size: 12px;
		font-weight: 600;
		min-width: 0;
	}
	.filter {
		min-width: 0;
	}
	select {
		display: block;
		width: 100%;
		margin-top: 5px;
	}
	.rent-boxes {
		list-style: none;
		padding: 0;
		margin: 0;
		border-top: 1px solid var(--hairline);
	}
	.rent-boxes li {
		min-width: 0;
		padding: 18px 0;
		border-bottom: 1px solid var(--hairline);
	}
	.box-title {
		display: flex;
		align-items: start;
		justify-content: space-between;
		gap: 18px;
		margin-bottom: 14px;
	}
	.box-id {
		font: 500 12px/1.5 var(--font-mono);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.category {
		font-size: 11px;
		color: var(--fg-muted);
		flex: 0 0 135px;
		text-align: right;
	}
	.mature .category {
		color: var(--warn-ink);
	}
	.box-facts {
		display: grid;
		grid-template-columns: 1.2fr 1fr 1fr;
		gap: 12px;
	}
	.box-facts > div {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.box-facts span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.box-facts strong {
		font-size: 13px;
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	details {
		margin-top: 12px;
		font-size: 12px;
	}
	summary {
		cursor: pointer;
		color: var(--fg-muted);
		width: fit-content;
	}
	details dl {
		padding-top: 12px;
		grid-template-columns: repeat(3, minmax(0, 1fr));
	}
	.box-pager {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin-top: 18px;
	}
	.box-pager span {
		font-size: 12px;
		text-align: center;
	}
	.empty,
	.ready {
		border: 1px dashed var(--hairline);
		padding: 24px;
		margin: 16px 0 0;
		color: var(--fg-muted);
		font-size: 13px;
	}
	:global([data-appearance='prism']) .rent-workspace {
		grid-template-columns: minmax(220px, 0.7fr) minmax(0, 1.6fr);
		gap: 28px;
	}
	:global([data-appearance='prism']) .rent-summary {
		border: 1px solid var(--hairline);
		padding: 22px;
		align-self: start;
		border-radius: 18px;
		background: var(--surface);
	}
	:global([data-appearance='prism']) .rent-summary > dl {
		grid-template-columns: 1fr;
	}
	:global([data-appearance='atelier']) .rent-workspace {
		grid-template-columns: minmax(0, 1fr) 240px;
	}
	:global([data-appearance='atelier']) .rent-summary {
		grid-column: 2;
		grid-row: 1;
		border-inline-start: 3px double var(--hairline);
		padding-inline-start: 22px;
	}
	:global([data-appearance='atelier']) .rent-evidence {
		grid-column: 1;
		grid-row: 1;
	}
	:global([data-appearance='atelier']) .rent-summary > dl {
		grid-template-columns: 1fr;
	}
	:global([data-appearance='atelier']) .rent-head {
		border-bottom: 3px double var(--hairline);
		padding-bottom: 18px;
	}
	:global([data-appearance='atelier']) .rent-boxes li {
		padding: 14px 0;
	}
	:global([data-appearance='aurora']) .rent-summary {
		display: grid;
		grid-template-columns: 1fr 1.2fr;
		gap: 14px 40px;
		padding: 28px;
		border-block: 1px solid var(--hairline);
		background: var(--surface);
	}
	:global([data-appearance='aurora']) .rent-summary .eyebrow {
		grid-column: 1/-1;
	}
	:global([data-appearance='aurora']) .rent-summary > dl {
		grid-template-columns: 1fr 1fr;
		border: 0;
		padding: 0;
	}
	:global([data-appearance='aurora']) .rent-summary .explanation {
		grid-column: 1/-1;
		margin: 0;
	}
	:global([data-appearance='aurora']) .rent-boxes {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 20px;
		border: 0;
	}
	:global([data-appearance='aurora']) .rent-boxes li {
		border: 1px solid var(--hairline);
		border-radius: 20px;
		padding: 24px;
		background: var(--surface);
	}
	:global([data-appearance='aurora']) .box-title {
		flex-direction: column;
		gap: 8px;
	}
	:global([data-appearance='aurora']) .category {
		flex-basis: auto;
		text-align: left;
	}
	@media (max-width: 1050px) {
		:global([data-appearance='aurora']) .rent-boxes {
			grid-template-columns: 1fr;
		}
	}
	@media (max-width: 700px) {
		.rent-head {
			flex-direction: column;
			gap: 12px;
		}
		.load {
			width: 100%;
		}
		.rent-summary > dl {
			grid-template-columns: 1fr 1fr;
		}
		:global([data-appearance]) .rent-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		:global([data-appearance='atelier']) .rent-summary,
		:global([data-appearance='atelier']) .rent-evidence {
			grid-column: auto;
			grid-row: auto;
		}
		:global([data-appearance='atelier']) .rent-summary {
			border-inline-start: 0;
			border-bottom: 3px double var(--hairline);
			padding: 0 0 16px;
		}
		:global([data-appearance='aurora']) .rent-summary {
			grid-template-columns: 1fr;
			padding: 20px;
		}
		:global([data-appearance='aurora']) .rent-boxes li {
			padding: 18px;
		}
		.box-title {
			flex-direction: column;
			gap: 8px;
		}
		.category {
			flex-basis: auto;
			text-align: left;
		}
		.box-facts,
		details dl {
			grid-template-columns: 1fr 1fr;
		}
	}
	@media (max-width: 400px) {
		.filters {
			grid-template-columns: 1fr;
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
