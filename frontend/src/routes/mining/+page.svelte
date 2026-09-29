<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { status } from '$lib/status/status.svelte';
	import { formatErg } from '$lib/format/amount';
	import {
		createMiningLoader,
		miningQuery,
		range,
		shareWidth,
		type MiningState
	} from '$lib/mining/overview';
	let from = $state('');
	let to = $state('');
	let top = $state('20');
	let pin = $state<string | undefined>();
	let localError = $state<string | null>(null);
	let mining = $state<MiningState>({ loading: false, data: null, error: null });
	const loader = createMiningLoader((next) => {
		mining = next;
		if (next.data) pin = next.data.anchor.block_id;
	});
	const share = $derived(
		mining.data
			? '/mining?' +
					miningQuery({
						from_height: mining.data.from_height,
						to_height: mining.data.to_height,
						top: mining.data.top,
						end_block_id: mining.data.anchor.block_id
					})
			: null
	);
	const network = $derived(
		mining.data
			? '/network?' +
					new URLSearchParams({
						from_height: String(mining.data.from_height),
						to_height: String(mining.data.to_height),
						buckets: '60',
						end_block_id: mining.data.anchor.block_id
					})
			: '/network'
	);
	$effect(() => {
		const query = page.url.searchParams;
		loader.clear();
		localError = null;
		from = query.get('from_height') ?? '';
		to = query.get('to_height') ?? '';
		top = query.get('top') ?? '20';
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
			void loader.load(range(from.trim(), to.trim(), top.trim(), pin));
		} catch (error) {
			loader.clear();
			localError = error instanceof Error ? error.message : 'Check the selected range.';
		}
	}
	const exact = (value: number) => value.toLocaleString('en-US');
</script>

<svelte:head
	><title>Mining signals — Kadia Explorer</title><meta
		name="description"
		content="Inspect exact block shares by miner public key, header versions and observed vote bytes across a pinned Ergo block range."
	/></svelte:head
>
<div class="mining-page">
	<header class="mining-intro">
		<p class="eyebrow">MINING / OBSERVED HEADER SIGNALS</p>
		<h1>The keys behind the blocks.<span>Start with the evidence.</span></h1>
		<p>
			Explore the public keys, versions and vote bytes recorded in an exact range of indexed blocks.
			A public key is not a verified miner, pool or owner.
		</p>
	</header>
	<form onsubmit={load} aria-label="Mining height range">
		<div class="range-fields">
			<label
				>From height<input
					inputmode="numeric"
					maxlength="10"
					bind:value={from}
					oninput={changed}
					required
				/></label
			>
			<label
				>To height<input
					inputmode="numeric"
					maxlength="10"
					bind:value={to}
					oninput={changed}
					required
				/></label
			>
			<label
				>Top entries<input
					inputmode="numeric"
					maxlength="2"
					bind:value={top}
					oninput={changed}
					required
				/></label
			>
		</div>
		<div class="range-actions">
			<button type="submit" disabled={mining.loading}
				>{mining.loading ? 'Loading signals…' : 'Load mining signals'}</button
			>
			<button type="button" class="secondary" onclick={recent} disabled={!status.current?.indexed}
				>Use latest 720 blocks</button
			>
			{#if pin}<button type="button" class="secondary" onclick={changed}>Clear end-block pin</button
				>{/if}
		</div>
		<p>
			Up to 20,160 blocks and 50 entries per ranking. All selected blocks contribute to every share;
			omitted entries have an exact remainder. Loading is explicit, with no background refresh.
		</p>
		{#if pin}<p class="pin">
				Pinned end block: <a href={'/blocks/' + pin}>{pin}</a>. Refresh rejects a changed canonical
				block until you clear the pin.
			</p>{/if}
	</form>
	{#if localError || mining.error}<div role="alert" class="notice">
			<h2>Range unavailable</h2>
			<p>{localError ?? mining.error}</p>
			<p>No partial result or earlier snapshot is shown.</p>
		</div>{/if}
	{#if mining.data}
		{@const data = mining.data}
		<section aria-label="Mining signals results" class="mining-results">
			<div class="range-heading">
				<div>
					<p class="eyebrow">COMPLETE SELECTED RANGE</p>
					<h2>{exact(data.from_height)} → {exact(data.to_height)}</h2>
				</div>
				<div class="result-links">
					<a href={share!}>Open pinned range ↗</a><a href={network}
						>Network activity for this range ↗</a
					>
				</div>
			</div>
			<dl class="summary">
				<div>
					<dt>Canonical blocks</dt>
					<dd>{exact(data.block_count)}</dd>
				</div>
				<div>
					<dt>Distinct public keys</dt>
					<dd>{exact(data.miner_keys.distinct_count)}</dd>
				</div>
				<div>
					<dt>Header versions</dt>
					<dd>{exact(data.versions.length)}</dd>
				</div>
				<div>
					<dt>Known vote fields</dt>
					<dd>{exact(data.votes.known_blocks)} <small>/ {exact(data.block_count)}</small></dd>
				</div>
			</dl>
			<section class="key-panel" aria-label="Public key block shares">
				<div class="section-heading">
					<p class="eyebrow">01 / BLOCK PRODUCTION</p>
					<h3>Public keys in the headers</h3>
					<p>
						Each share is an exact count / {exact(data.block_count)} selected blocks. Different keys may
						be controlled by the same entity; these are not ownership or pool shares.
					</p>
				</div>
				<ol class="key-ranking">
					{#each data.miner_keys.items as key, index (key.public_key)}
						<li>
							<div class="key-head">
								<span class="rank">{String(index + 1).padStart(2, '0')}</span><code
									>{key.public_key}</code
								><strong>{exact(key.block_count)} / {exact(data.block_count)}</strong>
							</div>
							<div class="share-track" aria-hidden="true">
								<span style:width={shareWidth(key.block_count, data.block_count) + '%'}></span>
							</div>
							<div class="key-evidence">
								<span
									>First <a href={'/blocks/' + key.first_block_id}>{exact(key.first_height)}</a> ·
									Last <a href={'/blocks/' + key.last_block_id}>{exact(key.last_height)}</a></span
								><span title={key.fees + ' nanoERG'}>{formatErg(key.fees)} ERG in fee outputs</span>
							</div>
						</li>
					{/each}
				</ol>
				{#if data.miner_keys.other_key_count > 0}<div class="remainder">
						<strong>Other {exact(data.miner_keys.other_key_count)} public keys</strong><span
							>{exact(data.miner_keys.other_block_count)} / {exact(data.block_count)} blocks</span
						>
						<p>{formatErg(data.miner_keys.other_fees)} ERG in fee outputs in those blocks.</p>
					</div>{/if}
				<p class="fee-note">
					The whole range contains {formatErg(data.totals.fees)} ERG in transaction fee outputs (<span
						class="raw">{data.totals.fees} nanoERG</span
					>). Grouping those outputs by header key does not prove who collected them. Subsidy,
					storage rent and total miner earnings are not calculated here.
				</p>
			</section>
			<div class="signal-panels">
				<section class="version-panel" aria-label="Header versions">
					<p class="eyebrow">02 / VERSION SIGNALS</p>
					<h3>Versions actually observed</h3>
					<p>Version bytes recorded in headers, without inferring an activated protocol rule.</p>
					<ul class="signal-list">
						{#each data.versions as item (item.version)}<li>
								<strong>Version {item.version}</strong><span
									>{exact(item.block_count)} / {exact(data.block_count)}</span
								>
								<div class="share-track" aria-hidden="true">
									<span style:width={shareWidth(item.block_count, data.block_count) + '%'}></span>
								</div>
							</li>{/each}
					</ul>
				</section>
				<section class="vote-panel" aria-label="Observed vote bytes">
					<p class="eyebrow">03 / RAW VOTE BYTES</p>
					<h3>Signals, without assumptions</h3>
					<p>
						Each tuple shows the three header vote bytes in hexadecimal. Tuple counts are not votes
						for one proposal, approval thresholds or activated parameters.
					</p>
					<ul class="signal-list">
						{#each data.votes.items as item (item.votes)}<li>
								<code>{item.votes.match(/../g)?.join(' ')}</code><span
									>{exact(item.block_count)} / {exact(data.block_count)}</span
								>
								<div class="share-track" aria-hidden="true">
									<span style:width={shareWidth(item.block_count, data.block_count) + '%'}></span>
								</div>
							</li>{/each}
					</ul>
					{#if data.votes.other_tuple_count > 0}<p class="remainder">
							Other {exact(data.votes.other_tuple_count)} tuples: {exact(
								data.votes.other_block_count
							)} / {exact(data.block_count)} blocks.
						</p>{/if}
					<p class="vote-coverage">
						<strong>{exact(data.votes.unknown_blocks)} unknown vote fields.</strong> Missing or
						malformed fields are counted here, never as zero votes. The <code>00 00 00</code> tuple
						appears in {exact(data.votes.zero_vote_blocks)} blocks.
					</p>
				</section>
			</div>
			<aside class="coverage">
				<h3>What this snapshot establishes</h3>
				<p>
					All {exact(data.block_count)} requested canonical headers were read together, at indexed height
					{exact(data.indexed_height)}. Anchor:
					<a href={'/blocks/' + data.anchor.block_id}>{data.anchor.block_id}</a>.
				</p>
				<p>
					{data.full_history
						? 'The index reports full retained history.'
						: 'The index is partial' +
							(data.partial_from === null
								? '.'
								: ', from height ' + exact(data.partial_from) + '.')} Coverage here describes only the
					selected range. Complete header coverage does not guarantee that every raw vote field is usable.
				</p>
				<p>
					Shares are exact fractions. Bar widths are rounded down for display. This page does not
					estimate hashrate, identify pool operators or decide protocol governance outcomes.
				</p>
			</aside>
		</section>
	{:else if !mining.loading && !localError && !mining.error}<section class="empty">
			<h2>Choose the blocks to inspect.</h2>
			<p>
				Use a retained height range or prepare the latest 720 indexed blocks. Pinned links preserve
				the end-block identity and ask each viewer to load explicitly.
			</p>
			<a href="/network">Explore network activity ↗</a>
		</section>{/if}
</div>

<style>
	.mining-page {
		width: 100%;
		min-width: 0;
		max-width: 1440px;
		margin-inline: auto;
	}
	.mining-intro {
		max-width: 78ch;
		margin-block: 4px 32px;
	}
	.eyebrow {
		color: var(--accent-ink);
		font: 10px var(--font-mono);
		letter-spacing: 0.1em;
		margin-bottom: 12px;
	}
	h1 {
		font-size: clamp(32px, 4.5vw, 60px);
		line-height: 1.07;
		letter-spacing: -0.045em;
		margin-bottom: 18px;
	}
	h1 span {
		display: block;
		font-size: 0.6em;
		margin-top: 12px;
		color: var(--fg-muted);
		letter-spacing: -0.03em;
	}
	h2 {
		font-size: clamp(21px, 3vw, 32px);
		letter-spacing: -0.03em;
		overflow-wrap: anywhere;
	}
	h3 {
		font-size: 19px;
		letter-spacing: -0.025em;
		margin-bottom: 12px;
	}
	p {
		font-size: 12px;
		line-height: 1.8;
		color: var(--fg-muted);
	}
	form,
	.empty,
	.notice {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		padding: 24px;
		margin-block: 24px;
		min-width: 0;
	}
	.range-fields {
		display: grid;
		grid-template-columns: 1fr 1fr 0.6fr;
		gap: 16px;
	}
	label {
		display: grid;
		gap: 8px;
		min-width: 0;
		color: var(--fg-muted);
		font-size: 11px;
	}
	input {
		width: 100%;
		min-width: 0;
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		color: var(--fg);
		border-radius: var(--radius-control);
		font: 13px var(--font-mono);
		padding: 11px;
	}
	.range-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		margin-block: 18px 12px;
	}
	button {
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		padding: 11px 15px;
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
	.pin,
	.raw {
		overflow-wrap: anywhere;
	}
	.mining-results {
		min-width: 0;
	}
	.range-heading {
		display: flex;
		gap: 20px;
		justify-content: space-between;
		align-items: center;
		margin-block: 36px 28px;
	}
	.result-links {
		display: grid;
		gap: 10px;
		font-size: 11px;
	}
	.summary {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 24px;
		padding: 24px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		margin-bottom: 28px;
	}
	.summary > div {
		min-width: 0;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 12px;
	}
	dd {
		font-size: clamp(24px, 3vw, 38px);
		letter-spacing: -0.04em;
		overflow-wrap: anywhere;
	}
	dd small {
		font: 12px var(--font-mono);
		color: var(--fg-muted);
	}
	.key-panel,
	.version-panel,
	.vote-panel {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		padding: 26px;
		min-width: 0;
	}
	.section-heading {
		max-width: 80ch;
		margin-bottom: 24px;
	}
	.key-ranking,
	.signal-list {
		list-style: none;
		padding: 0;
		margin: 0;
	}
	.key-ranking li {
		padding-block: 18px;
		border-top: 1px solid var(--hairline);
		min-width: 0;
	}
	.key-head {
		display: grid;
		grid-template-columns: 28px minmax(0, 1fr) auto;
		gap: 12px;
		align-items: start;
	}
	.rank {
		color: var(--fg-muted);
		font: 11px var(--font-mono);
	}
	code {
		font: 11px var(--font-mono);
		overflow-wrap: anywhere;
		line-height: 1.7;
	}
	.key-head strong {
		font: 13px var(--font-mono);
		white-space: nowrap;
	}
	.share-track {
		height: 5px;
		border-radius: 5px;
		background: var(--accent-wash);
		overflow: hidden;
		margin-block: 12px;
	}
	.share-track > span {
		display: block;
		height: 100%;
		background: var(--accent-ink);
	}
	.key-evidence {
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		justify-content: space-between;
		font: 10px var(--font-mono);
		color: var(--fg-muted);
		line-height: 1.8;
		overflow-wrap: anywhere;
	}
	.remainder {
		padding: 16px;
		border: 1px dashed var(--hairline);
		border-radius: var(--radius-control);
		margin-block: 16px;
		font-size: 12px;
		display: flex;
		gap: 8px 16px;
		flex-wrap: wrap;
		justify-content: space-between;
	}
	.remainder p {
		flex-basis: 100%;
	}
	.fee-note {
		margin-top: 20px;
		border-top: 1px solid var(--hairline);
		padding-top: 18px;
	}
	.signal-panels {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: 24px;
		margin-top: 24px;
		min-width: 0;
	}
	.signal-list {
		margin-top: 24px;
	}
	.signal-list li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 0 12px;
		padding-block: 10px;
		font-size: 12px;
	}
	.signal-list li > span {
		font: 11px var(--font-mono);
	}
	.signal-list .share-track {
		grid-column: 1/-1;
	}
	.vote-coverage {
		padding-top: 16px;
		border-top: 1px solid var(--hairline);
	}
	.coverage {
		margin-top: 28px;
		border-top: 1px solid var(--hairline);
		padding-top: 24px;
		min-width: 0;
	}
	.coverage p {
		margin-block: 10px;
	}
	:global([data-appearance='aurora']) .mining-intro {
		margin: 36px auto 44px;
		text-align: center;
	}
	:global([data-appearance='aurora']) form {
		max-width: 1050px;
		margin-inline: auto;
		border-radius: 28px;
		box-shadow: 0 18px 50px color-mix(in srgb, var(--accent-ink) 8%, transparent);
	}
	:global([data-appearance='aurora']) .summary {
		border: 0;
		background: transparent;
		text-align: center;
		margin-block: 36px;
	}
	:global([data-appearance='aurora']) .key-panel {
		margin-inline: 36px;
		border-radius: 32px;
		box-shadow: 0 24px 64px color-mix(in srgb, var(--accent-ink) 8%, transparent);
		padding: 36px;
	}
	:global([data-appearance='aurora']) .key-ranking {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0 32px;
	}
	:global([data-appearance='aurora']) .key-head {
		grid-template-columns: 24px minmax(0, 1fr);
	}
	:global([data-appearance='aurora']) .key-head strong {
		grid-column: 2;
	}
	:global([data-appearance='aurora']) .signal-panels {
		margin-inline: 72px;
	}
	:global([data-appearance='aurora']) .version-panel,
	:global([data-appearance='aurora']) .vote-panel {
		border: 0;
		background: transparent;
		padding-block: 32px;
	}
	:global([data-appearance='atelier']) form {
		border-inline: 0;
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .mining-results {
		display: grid;
		grid-template-columns: minmax(140px, 0.25fr) minmax(0, 1fr);
		gap: 28px;
		border-top: 3px double var(--fg);
	}
	:global([data-appearance='atelier']) .range-heading {
		grid-column: 1/-1;
		margin-bottom: 0;
	}
	:global([data-appearance='atelier']) .summary {
		grid-column: 1;
		grid-row: 2;
		grid-template-columns: 1fr;
		align-content: start;
		margin: 0;
		border: 0;
		border-radius: 0;
		background: transparent;
		padding: 0;
	}
	:global([data-appearance='atelier']) .key-panel {
		grid-column: 2;
		grid-row: 2;
		border: 0;
		border-left: 1px solid var(--hairline);
		border-radius: 0;
		background: transparent;
		padding-block: 0;
	}
	:global([data-appearance='atelier']) .signal-panels {
		grid-column: 2;
		display: block;
		margin: 0;
	}
	:global([data-appearance='atelier']) .version-panel,
	:global([data-appearance='atelier']) .vote-panel {
		border: 0;
		border-top: 1px solid var(--hairline);
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .coverage {
		grid-column: 1/-1;
	}
	:global([data-appearance='prism']) .mining-results {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(260px, 0.48fr);
		gap: 24px;
	}
	:global([data-appearance='prism']) .range-heading,
	:global([data-appearance='prism']) .summary {
		grid-column: 1/-1;
		margin-bottom: 0;
	}
	:global([data-appearance='prism']) .summary {
		border-radius: 20px;
		background: var(--accent-wash);
	}
	:global([data-appearance='prism']) .key-panel {
		grid-column: 1;
		border-radius: 24px;
	}
	:global([data-appearance='prism']) .signal-panels {
		grid-column: 2;
		display: flex;
		flex-direction: column;
		gap: 20px;
		margin: 0;
	}
	:global([data-appearance='prism']) .version-panel,
	:global([data-appearance='prism']) .vote-panel {
		border-radius: 24px;
	}
	:global([data-appearance='prism']) .coverage {
		grid-column: 1/-1;
		margin: 0;
	}
	@media (max-width: 900px) {
		.range-heading {
			flex-direction: column;
			align-items: start;
		}
		.summary {
			grid-template-columns: 1fr 1fr;
		}
		.signal-panels {
			grid-template-columns: 1fr;
		}
		:global([data-appearance='prism']) .mining-results,
		:global([data-appearance='atelier']) .mining-results {
			display: block;
		}
		:global([data-appearance='prism']) .summary,
		:global([data-appearance='atelier']) .summary {
			display: grid;
			grid-template-columns: 1fr 1fr;
			margin-block: 24px;
		}
		:global([data-appearance='prism']) .signal-panels,
		:global([data-appearance='atelier']) .signal-panels {
			margin-top: 24px;
		}
		:global([data-appearance='atelier']) .key-panel {
			border-left: 0;
			padding-inline: 0;
		}
		:global([data-appearance='aurora']) .key-panel,
		:global([data-appearance='aurora']) .signal-panels {
			margin-inline: 0;
		}
		:global([data-appearance='aurora']) .key-ranking {
			display: block;
		}
		:global([data-appearance='prism']) .coverage {
			margin-top: 24px;
		}
	}
	@media (max-width: 560px) {
		form,
		.empty,
		.notice,
		.key-panel,
		.version-panel,
		.vote-panel,
		:global([data-appearance='aurora']) .key-panel {
			padding: 18px;
		}
		.range-fields {
			grid-template-columns: 1fr 1fr;
		}
		.range-fields label:last-child {
			grid-column: 1/-1;
		}
		.key-head {
			grid-template-columns: 20px minmax(0, 1fr);
		}
		.key-head strong {
			grid-column: 2;
		}
		.summary {
			padding: 18px;
			gap: 24px 14px;
		}
		.key-evidence {
			flex-direction: column;
			gap: 6px;
		}
	}
</style>
