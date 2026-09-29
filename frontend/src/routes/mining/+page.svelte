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
		<h1>Mining signals</h1>
		<p>Header keys, versions and raw vote bytes.</p>
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
			<button
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
				<button type="button" class="secondary" onclick={changed}>Clear end-block pin</button>
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
					<h2>{exact(data.from_height)} → {exact(data.to_height)}</h2>
				</div>
				<div class="result-links">
					<a href={share!} aria-label="Open pinned range">Pinned link ↗</a><a
						href={network}
						aria-label="Network activity for this range">Network ↗</a
					>
				</div>
			</div>
			<p class="coverage-strip">
				{data.full_history
					? 'Full retained history.'
					: 'The index is partial' +
						(data.partial_from === null ? '.' : ', from height ' + exact(data.partial_from) + '.')}
				All {exact(data.block_count)} requested headers present · {exact(data.votes.unknown_blocks)} unknown
				vote fields.
			</p>
			<dl class="summary">
				<div>
					<dt>Blocks</dt>
					<dd>{exact(data.block_count)}</dd>
				</div>
				<div>
					<dt>Public keys</dt>
					<dd>{exact(data.miner_keys.distinct_count)}</dd>
				</div>
				<div>
					<dt>Versions</dt>
					<dd>{exact(data.versions.length)}</dd>
				</div>
				<div>
					<dt>Votes known</dt>
					<dd>{exact(data.votes.known_blocks)} <small>/ {exact(data.block_count)}</small></dd>
				</div>
			</dl>
			<section class="key-panel" aria-label="Public key block shares">
				<div class="section-heading">
					<h3>Block shares by public key</h3>
					<p>
						Counts / {exact(data.block_count)} blocks. Keys are not verified owners.
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
					Range fee outputs: {formatErg(data.totals.fees)} ERG (<span class="raw"
						>{data.totals.fees} nanoERG</span
					>). This is not proven miner income.
				</p>
			</section>
			<div class="signal-panels">
				<section class="version-panel" aria-label="Header versions">
					<h3>Header versions</h3>
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
					<h3>Raw vote bytes</h3>
					<p>Three-byte tuples, not activated parameters.</p>
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
			<details class="coverage">
				<summary>Methodology and snapshot evidence</summary>
				<p>
					Up to 20,160 blocks and 50 entries per ranking. Loading is explicit, with no background
					refresh. Refresh preserves the end-block pin and rejects a changed canonical block until
					you clear it.
				</p>
				<p>
					All {exact(data.block_count)} requested canonical headers were read together, at indexed height
					{exact(data.indexed_height)}. Anchor:
					<a href={'/blocks/' + data.anchor.block_id}>{data.anchor.block_id}</a>.
				</p>
				<p>
					Coverage here describes only the selected range. Complete header coverage does not
					guarantee that every raw vote field is usable.
				</p>
				<p>
					Shares are exact fractions. Bar widths are rounded down for display. This page does not
					estimate hashrate, identify pool operators or decide protocol governance outcomes.
				</p>
				<p>
					Different public keys may be controlled by the same entity. Vote tuples do not establish
					proposal approval or activated protocol rules. Fee outputs grouped by header key do not
					prove who collected them; subsidy, storage rent and total earnings are not calculated.
				</p>
			</details>
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
		margin-bottom: var(--density-gap, 16px);
	}
	.eyebrow {
		color: var(--accent-ink);
		font: 10px var(--font-mono);
		letter-spacing: 0.08em;
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
	h3 {
		font-size: 15px;
		line-height: 1.4;
		margin: 0 0 4px;
	}
	p {
		font-size: 12px;
		line-height: 1.6;
		color: var(--fg-muted);
		margin: 0;
	}
	form,
	.empty,
	.notice {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
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
		min-width: 0;
		color: var(--fg-muted);
		font-size: 11px;
	}
	input {
		width: 100%;
		min-width: 0;
		min-height: var(--density-row, 44px);
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		color: var(--fg);
		border-radius: var(--radius-control);
		font: 13px var(--font-mono);
		padding: 7px 8px;
	}
	.range-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	button {
		min-height: var(--density-row, 44px);
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		padding: 7px 10px;
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
	.raw {
		overflow-wrap: anywhere;
	}
	.mining-results {
		min-width: 0;
	}
	.range-heading {
		display: flex;
		flex-wrap: wrap;
		gap: 4px var(--density-gap, 16px);
		justify-content: space-between;
		align-items: center;
		margin-bottom: 6px;
	}
	.result-links {
		display: flex;
		gap: 12px;
		font-size: 11px;
	}
	.coverage-strip {
		padding-bottom: 8px;
		font-size: 11px;
	}
	.summary {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--density-gap, 16px);
		padding: 10px var(--density-panel, 16px);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		margin: 0 0 var(--density-gap, 16px);
	}
	.summary > div {
		min-width: 0;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin: 0 0 3px;
	}
	dd {
		margin: 0;
		font-size: 23px;
		line-height: 1.3;
		letter-spacing: -0.035em;
		overflow-wrap: anywhere;
	}
	dd small {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	.key-panel,
	.version-panel,
	.vote-panel {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		padding: var(--density-panel, 16px);
		min-width: 0;
	}
	.section-heading {
		margin-bottom: 8px;
	}
	.key-ranking,
	.signal-list {
		list-style: none;
		padding: 0;
		margin: 0;
	}
	.key-ranking li {
		padding-block: calc(var(--density-gap, 16px) / 2);
		border-top: 1px solid var(--hairline);
		min-width: 0;
	}
	.key-head {
		display: grid;
		grid-template-columns: 24px minmax(0, 1fr) auto;
		gap: 8px;
		align-items: start;
	}
	.rank {
		color: var(--fg-muted);
		font: 11px var(--font-mono);
		line-height: 1.7;
	}
	code {
		font: 12px/1.6 var(--font-mono);
		overflow-wrap: anywhere;
	}
	.key-head strong {
		font: 13px/1.6 var(--font-mono);
		white-space: nowrap;
	}
	.share-track {
		height: 4px;
		border-radius: 4px;
		background: var(--accent-wash);
		overflow: hidden;
		margin-block: 6px;
	}
	.share-track > span {
		display: block;
		height: 100%;
		background: var(--accent-ink);
	}
	.key-evidence {
		display: flex;
		gap: 4px 12px;
		flex-wrap: wrap;
		justify-content: space-between;
		font: 11px/1.5 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.remainder {
		padding: 8px;
		border: 1px dashed var(--hairline);
		border-radius: var(--radius-control);
		margin-block: 8px;
		font-size: 12px;
		display: flex;
		gap: 4px 12px;
		flex-wrap: wrap;
		justify-content: space-between;
	}
	.remainder p {
		flex-basis: 100%;
	}
	.fee-note {
		margin-top: 8px;
		border-top: 1px solid var(--hairline);
		padding-top: 8px;
		font-size: 11px;
	}
	.signal-panels {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: var(--density-gap, 16px);
		margin-top: var(--density-gap, 16px);
		min-width: 0;
	}
	.signal-list {
		margin-top: 8px;
	}
	.signal-list li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 0 8px;
		padding-block: 5px;
		font-size: 12px;
	}
	.signal-list li > span {
		font: 11px var(--font-mono);
	}
	.signal-list .share-track {
		grid-column: 1/-1;
	}
	.vote-coverage {
		padding-top: 8px;
		border-top: 1px solid var(--hairline);
	}
	.coverage {
		margin-top: var(--density-gap, 16px);
		border-top: 1px solid var(--hairline);
		min-width: 0;
	}
	summary {
		cursor: pointer;
		font-size: 12px;
		padding-block: 10px;
	}
	.coverage p {
		margin-block: 8px;
	}
	:global([data-appearance='aurora']) .mining-intro {
		text-align: center;
	}
	:global([data-appearance='aurora']) form {
		border-radius: 18px;
		box-shadow: 0 12px 32px color-mix(in srgb, var(--accent-ink) 8%, transparent);
	}
	:global([data-appearance='aurora']) .summary {
		border: 0;
		background: transparent;
		text-align: center;
	}
	:global([data-appearance='aurora']) .key-panel {
		margin-inline: 12px;
		border-radius: 20px;
		box-shadow: 0 16px 40px color-mix(in srgb, var(--accent-ink) 8%, transparent);
	}
	:global([data-appearance='aurora']) .key-ranking {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0 20px;
	}
	:global([data-appearance='aurora']) .key-head {
		grid-template-columns: 20px minmax(0, 1fr);
	}
	:global([data-appearance='aurora']) .key-head strong {
		grid-column: 2;
	}
	:global([data-appearance='aurora']) .signal-panels {
		margin-inline: 24px;
	}
	:global([data-appearance='aurora']) .version-panel,
	:global([data-appearance='aurora']) .vote-panel {
		border: 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) form {
		border-inline: 0;
		border-radius: 0;
		background: transparent;
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .mining-results {
		display: grid;
		grid-template-columns: minmax(120px, 0.2fr) minmax(0, 1fr);
		gap: 8px var(--density-gap, 16px);
		border-top: 3px double var(--fg);
		padding-top: 8px;
	}
	:global([data-appearance='atelier']) .range-heading,
	:global([data-appearance='atelier']) .coverage-strip {
		grid-column: 1/-1;
		margin: 0;
	}
	:global([data-appearance='atelier']) .summary {
		grid-column: 1;
		grid-row: 3;
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
		grid-row: 3;
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
		grid-template-columns: minmax(0, 1fr) minmax(250px, 0.48fr);
		gap: 8px var(--density-gap, 16px);
	}
	:global([data-appearance='prism']) .range-heading,
	:global([data-appearance='prism']) .coverage-strip,
	:global([data-appearance='prism']) .summary {
		grid-column: 1/-1;
		margin: 0;
	}
	:global([data-appearance='prism']) .summary {
		border-radius: 14px;
		background: var(--accent-wash);
	}
	:global([data-appearance='prism']) .key-panel {
		grid-column: 1;
		border-radius: 16px;
	}
	:global([data-appearance='prism']) .signal-panels {
		grid-column: 2;
		display: flex;
		flex-direction: column;
		gap: var(--density-gap, 16px);
		margin: 0;
	}
	:global([data-appearance='prism']) .version-panel,
	:global([data-appearance='prism']) .vote-panel {
		border-radius: 16px;
	}
	:global([data-appearance='prism']) .coverage {
		grid-column: 1/-1;
		margin: 0;
	}
	@media (max-width: 900px) {
		form {
			grid-template-columns: 1fr;
		}
		.range-actions {
			display: grid;
			grid-template-columns: 1fr 1fr;
		}
		button,
		input {
			min-height: 44px;
		}
		.pin button {
			min-height: 44px;
		}
		.pin a,
		.result-links a,
		.key-evidence a {
			display: inline-flex;
			align-items: center;
			min-height: 44px;
		}
		.key-evidence a {
			min-width: 44px;
			justify-content: center;
		}
		summary {
			min-height: 44px;
			padding-block: 12px;
		}
		.range-heading {
			gap: 0 12px;
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
			grid-template-columns: repeat(4, minmax(0, 1fr));
			margin-block: 0 var(--density-gap, 16px);
		}
		:global([data-appearance='prism']) .signal-panels,
		:global([data-appearance='atelier']) .signal-panels {
			margin-top: var(--density-gap, 16px);
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
			margin-top: var(--density-gap, 16px);
		}
	}
	@media (max-width: 560px) {
		.range-fields {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(58px, 0.6fr);
		}
		.key-head {
			grid-template-columns: 20px minmax(0, 1fr);
		}
		.key-head strong {
			grid-column: 2;
		}
		.summary {
			gap: 8px;
			padding-inline: 8px;
		}
		dd {
			font-size: 20px;
		}
		.key-evidence {
			gap: 0;
			flex-direction: column;
		}
		.pin {
			flex-wrap: wrap;
		}
	}
</style>
