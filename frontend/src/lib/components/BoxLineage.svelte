<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api } from '$lib/api/endpoints';
	import type { BoxDto } from '$lib/api/types';
	import {
		createLineage,
		hasProducer,
		initialLineage,
		INITIAL_LINEAGE_ROWS,
		MAX_VISIBLE_LINEAGE_ROWS,
		type BoxReference,
		type LineageSide
	} from '$lib/boxes/lineage';
	import { formatErg, formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Icon from './Icon.svelte';
	let { box }: { box: BoxDto } = $props();
	let branches = $state(initialLineage());
	let expanded = $state<Record<string, boolean>>({});
	let refreshing = $state(false);
	let refreshError = $state('');
	let controller: ReturnType<typeof createLineage> | undefined;
	$effect(() => {
		const current = box;
		branches = initialLineage();
		expanded = {};
		const next = createLineage({
			box: current,
			getTx: (id) => api.tx(id),
			onChange: (value) => {
				branches = value;
			}
		});
		controller = next;
		return () => next.stop();
	});
	async function refresh() {
		refreshing = true;
		refreshError = '';
		try {
			await invalidateAll();
		} catch {
			refreshError = 'Box details could not be refreshed. Try again when the index is available.';
		} finally {
			refreshing = false;
		}
	}
	function visible(rows: BoxReference[], key: string) {
		return rows.slice(0, expanded[key] ? MAX_VISIBLE_LINEAGE_ROWS : INITIAL_LINEAGE_ROWS);
	}
</script>

{#snippet references(rows: BoxReference[], label: string, key: string)}
	<div class="reference-group">
		<h4>{label} <span>{rows.length}</span></h4>
		{#if !rows.length}<p class="muted">None in this transaction.</p>{:else}
			<ul class="references" aria-label={label}>
				{#each visible(rows, key) as row (row.id)}
					<li class:unresolved={!row.box}>
						<a class="box-link mono" href={`/box/${row.id}`} title={row.id}
							><Icon name="box" size={15} />{truncateMiddle(row.id, 9, 7)}<span class="sr-only">
								— open box</span
							></a
						>
						{#if row.box}<p class="box-value" title={`${row.box.value} nanoERG`}>
								{formatErg(row.box.value)} <span>ERG</span>
							</p>
							{#each row.box.tokens.slice(0, 2) as token, i (`${token.id}:${i}`)}<p
									class="token-line"
								>
									<span>{formatTokenAmount(token.amount, token.decimals)}</span>
									<a href={`/token/${token.id}`} title={token.id}
										><bdi>{token.name?.trim() || truncateMiddle(token.id, 6, 5)}</bdi></a
									>{#if token.decimals === null}<small>raw units</small>{/if}
								</p>{/each}
							{#if row.box.tokens.length > 2}<p class="muted">
									{row.box.tokens.length - 2} more tokens in box details.
								</p>{/if}
						{:else}<p class="unknown">Input value unavailable</p>
							<p class="muted">This input box is unresolved in the index.</p>{/if}
					</li>
				{/each}
			</ul>
			{#if rows.length > INITIAL_LINEAGE_ROWS}
				<button
					class="more"
					type="button"
					onclick={() => {
						expanded = { ...expanded, [key]: !expanded[key] };
					}}
					aria-expanded={!!expanded[key]}
					>{expanded[key]
						? `Show fewer ${label.toLowerCase()}`
						: `Show more ${label.toLowerCase()}`}</button
				>
			{/if}
			{#if rows.length > (expanded[key] ? MAX_VISIBLE_LINEAGE_ROWS : INITIAL_LINEAGE_ROWS)}<p
					class="limit-note"
				>
					Showing {visible(rows, key).length} of {rows.length} boxes.{#if expanded[key]}
						Open the transaction for the remaining references.{/if}
				</p>{/if}
		{/if}
	</div>
{/snippet}

{#snippet branch(side: LineageSide)}
	{@const producer = side === 'producer'}
	{@const available = producer ? hasProducer(box) : box.spent_by !== null}
	{@const txId = producer ? box.tx_id : box.spent_by}
	{@const item = branches[side]}
	<article
		class="branch {side}"
		aria-label={producer ? 'Producing transaction lineage' : 'Spending transaction lineage'}
	>
		<header class="branch-head">
			<p class="eyebrow">{producer ? '01 / Before this box' : '03 / After this box'}</p>
			<h3>{producer ? 'Produced by' : 'Consumed by'}</h3>
			{#if available && txId}<a class="transaction-link mono" href={`/tx/${txId}`} title={txId}
					>{truncateMiddle(txId, 10, 8)}<Icon name="arrow-right" size={15} /><span class="sr-only">
						— open {producer ? 'producing' : 'spending'} transaction</span
					></a
				>{/if}
		</header>
		<div class="branch-body">
			{#if !available}
				{#if producer}<p class="terminal">Genesis allocation</p>
					<p class="muted">
						The recorded zero transaction ID has no producing transaction. This is not an ordinary
						transaction output.
					</p>
				{:else}<p class="terminal">Unspent in this box record</p>
					<p class="muted">
						No spending transaction was recorded when these box details loaded. Refresh box details
						to recheck.
					</p>{/if}
			{:else if item.status === 'ready' && item.data}
				{@const data = item.data}
				<p class="inclusion">
					Included in <a href={`/blocks/${data.tx.block_id ?? data.tx.height}`}
						>block {data.tx.height.toLocaleString('en-US')}</a
					>.
				</p>
				<p class="snapshot">
					{#if data.tx.indexed_height != null}Transaction details checked at indexed block {data.tx.indexed_height.toLocaleString(
							'en-US'
						)}.{:else}Transaction detail snapshot height is unavailable.{/if}
				</p>
				{#if data.missingInputs}<p class="coverage">
						{data.missingInputs} unresolved input{data.missingInputs === 1 ? '' : 's'}. No complete
						input total is claimed.
					</p>{/if}
				{#if producer}
					{@render references(data.inputs, 'Previous boxes', 'previous')}
					{@render references(
						data.outputs.filter((row) => row.id !== box.id),
						'Sibling outputs',
						'siblings'
					)}
				{:else}
					{@render references(data.outputs, 'Following outputs', 'following')}
					<details class="other-inputs">
						<summary
							>Other inputs to this transaction ({data.inputs.filter((row) => row.id !== box.id)
								.length})</summary
						>{@render references(
							data.inputs.filter((row) => row.id !== box.id),
							'Other input boxes',
							'other'
						)}
					</details>
				{/if}
			{:else}
				{#if item.message}<p class="coverage" role="status">{item.message}</p>{:else}<p
						class="muted"
					>
						{producer
							? 'Inspect the transaction inputs and the other outputs created alongside this box.'
							: 'Inspect the outputs created by the transaction that consumed this box.'}
					</p>{/if}
				<button
					class="load"
					type="button"
					disabled={item.status === 'loading'}
					onclick={() => void controller?.load(side)}
					>{item.status === 'loading'
						? 'Loading transaction…'
						: `${item.status === 'idle' ? 'Inspect' : 'Retry'} ${producer ? 'producing' : 'spending'} transaction`}</button
				>
			{/if}
		</div>
	</article>
{/snippet}

<section class="box-lineage" aria-label="Box lineage">
	<header class="lineage-heading">
		<div>
			<p class="eyebrow">Indexed relationships</p>
			<h2>One box. Its immediate history.</h2>
			<p>
				Follow transaction inputs and outputs. These connections do not identify owners or show
				which payment moved between particular boxes.
			</p>
		</div>
		<button class="refresh" type="button" onclick={refresh} disabled={refreshing}
			>{refreshing ? 'Refreshing box…' : 'Refresh box details'}</button
		>
	</header>
	{#if refreshError}<p role="alert" class="coverage">{refreshError}</p>{/if}
	<div class="lineage-spread">
		{@render branch('producer')}
		<article class="focus-box" aria-label="Selected box">
			<div class="focus-symbol" aria-hidden="true"><Icon name="box" size={44} /></div>
			<p class="eyebrow">02 / Selected box</p>
			<h3>This box</h3>
			<p class="mono focus-id" title={box.id}>{truncateMiddle(box.id, 10, 8)}</p>
			<p class="focus-value" title={`${box.value} nanoERG`}>
				{formatErg(box.value)} <span>ERG</span>
			</p>
			<p class="muted">
				{box.tokens.length} token {box.tokens.length === 1 ? 'type' : 'types'} · output {box.index}
			</p>
			<p class="focus-note">
				Each box is consumed at most once. The surrounding transaction may also involve other boxes.
			</p>
		</article>
		{@render branch('spender')}
	</div>
	<p class="lineage-foot">
		Each side loads only when requested. Up to 20 references per list are shown; adjacent box links
		open a new inspector. Inclusion comes from the returned transaction, not the box’s declared
		creation height.
	</p>
</section>

<style>
	.box-lineage {
		min-width: 0;
		margin-block: 20px;
	}
	.lineage-heading {
		display: flex;
		justify-content: space-between;
		gap: 24px;
		align-items: start;
		margin-bottom: 26px;
	}
	.lineage-heading > div {
		min-width: 0;
		max-width: 76ch;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--accent-ink);
	}
	h2 {
		font: var(--weight-display, 750) clamp(25px, 3vw, 38px)/1.1
			var(--font-display, var(--font-sans));
		letter-spacing: -0.04em;
		margin-block: 10px 14px;
	}
	.lineage-heading p:last-child,
	.lineage-foot {
		font-size: 12px;
		line-height: 1.7;
		color: var(--fg-muted);
	}
	button {
		border: var(--rule);
		border-radius: 8px;
		color: var(--fg);
		background: var(--surface-solid);
		padding: 10px 13px;
		font-size: 12px;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.65;
		cursor: default;
	}
	.refresh {
		flex: none;
	}
	.lineage-spread {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(180px, 0.64fr) minmax(0, 1fr);
		grid-template-areas: 'producer focus spender';
		gap: 22px;
		align-items: start;
	}
	.branch {
		min-width: 0;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		overflow: hidden;
	}
	.producer {
		grid-area: producer;
	}
	.spender {
		grid-area: spender;
	}
	.branch-head {
		padding: 20px;
		border-bottom: var(--rule);
		background: var(--surface-hover);
	}
	h3 {
		font: var(--weight-display, 700) 23px var(--font-display, var(--font-sans));
		margin-block: 10px 14px;
	}
	.transaction-link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		font-size: 11px;
		color: var(--accent-ink);
		overflow-wrap: anywhere;
	}
	.branch-body {
		padding: 20px;
	}
	.muted,
	.snapshot,
	.limit-note,
	.focus-note {
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
	}
	.load {
		margin-top: 16px;
		width: 100%;
		background: var(--accent-wash);
		border-color: var(--accent-ink);
		color: var(--accent-ink);
	}
	.terminal {
		font-weight: 650;
		font-size: 14px;
		margin-bottom: 10px;
	}
	.inclusion {
		font-size: 12px;
		line-height: 1.6;
	}
	.inclusion a {
		color: var(--accent-ink);
		text-decoration: underline;
	}
	.snapshot {
		margin-top: 6px;
	}
	.coverage,
	.unknown {
		color: var(--warn-ink);
		font-size: 12px;
		line-height: 1.6;
		margin-block: 10px;
	}
	.reference-group {
		margin-top: 22px;
		min-width: 0;
	}
	h4 {
		display: flex;
		justify-content: space-between;
		gap: 10px;
		font-size: 12px;
		font-weight: 650;
		margin-bottom: 10px;
	}
	h4 span {
		font-family: var(--font-mono);
		color: var(--fg-muted);
	}
	.references {
		display: grid;
		gap: 9px;
	}
	.references li {
		border: var(--rule);
		border-left: 3px solid var(--accent-ink);
		padding: 12px;
		border-radius: 6px;
		background: var(--bg);
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.references li.unresolved {
		border-left-color: var(--warn-ink);
		border-style: dashed;
	}
	.box-link {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 11px;
		color: var(--accent-ink);
	}
	.box-value {
		margin-top: 9px;
		font: var(--weight-number, 650) 16px var(--font-number, var(--font-sans));
	}
	.box-value span {
		font: 11px var(--font-sans);
		color: var(--fg-muted);
	}
	.token-line {
		margin-top: 8px;
		font-size: 11px;
		line-height: 1.5;
	}
	.token-line a {
		margin-inline-start: 0.35em;
		text-decoration: underline;
	}
	.token-line small {
		display: block;
		color: var(--fg-muted);
	}
	.more {
		margin-top: 10px;
		padding: 7px 9px;
		font-size: 11px;
	}
	.limit-note {
		margin-top: 8px;
	}
	.other-inputs {
		margin-top: 20px;
		font-size: 12px;
	}
	.other-inputs summary {
		cursor: pointer;
		line-height: 1.6;
	}
	.focus-box {
		grid-area: focus;
		min-width: 0;
		position: relative;
		text-align: center;
		padding: 28px 16px;
		border-block: 2px solid var(--accent-ink);
		background: var(--accent-wash);
		margin-top: 66px;
	}
	.focus-box::before,
	.focus-box::after {
		content: '';
		position: absolute;
		width: 22px;
		height: 1px;
		top: 43px;
		background: var(--accent-ink);
	}
	.focus-box::before {
		right: 100%;
	}
	.focus-box::after {
		left: 100%;
	}
	.focus-symbol {
		display: flex;
		justify-content: center;
		color: var(--accent-ink);
		margin-bottom: 20px;
	}
	.focus-id {
		font-size: 11px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.focus-value {
		font: var(--weight-number, 750) clamp(20px, 2.2vw, 28px) var(--font-number, var(--font-sans));
		margin-block: 22px 8px;
		overflow-wrap: anywhere;
	}
	.focus-value span {
		font: 11px var(--font-sans);
	}
	.focus-note {
		margin-top: 22px;
	}
	.lineage-foot {
		margin-top: 22px;
	}
	:global(:root[data-appearance='prism']) .lineage-spread {
		grid-template-columns: repeat(2, minmax(0, 1fr));
		grid-template-areas: 'focus focus' 'producer spender';
		gap: 28px;
	}
	:global(:root[data-appearance='prism']) .focus-box {
		width: min(100%, 480px);
		justify-self: center;
		margin-top: 0;
		border: var(--rule);
		border-radius: 20px;
		background: linear-gradient(145deg, var(--surface-solid), var(--accent-wash));
		box-shadow:
			var(--shadow-lift),
			inset 0 1px 0 color-mix(in srgb, var(--fg), transparent 85%);
	}
	:global(:root[data-appearance='prism']) .focus-symbol {
		filter: drop-shadow(0 8px 8px color-mix(in srgb, var(--accent-ink), transparent 60%));
		transform: rotate(-9deg);
	}
	:global(:root[data-appearance='prism']) .focus-box::before,
	:global(:root[data-appearance='prism']) .focus-box::after {
		width: 1px;
		height: 29px;
		top: 100%;
		right: 25%;
		left: auto;
	}
	:global(:root[data-appearance='prism']) .focus-box::after {
		right: 75%;
	}
	:global(:root[data-appearance='prism']) .branch {
		box-shadow: var(--shadow-rest);
		border-radius: 14px;
	}
	:global(:root[data-appearance='atelier']) .lineage-heading {
		border-block: 3px double var(--fg);
		padding-block: 20px;
	}
	:global(:root[data-appearance='atelier']) .lineage-spread {
		grid-template-columns: minmax(0, 1fr);
		grid-template-areas: 'producer' 'focus' 'spender';
		gap: 0;
	}
	:global(:root[data-appearance='atelier']) .branch {
		display: grid;
		grid-template-columns: minmax(150px, 0.65fr) minmax(0, 2fr);
		border: 0;
		border-bottom: var(--rule);
		border-radius: 0;
		background: none;
	}
	:global(:root[data-appearance='atelier']) .branch-head {
		background: none;
		border: 0;
		padding-left: 0;
	}
	:global(:root[data-appearance='atelier']) .branch-body {
		border-left: var(--rule);
	}
	:global(:root[data-appearance='atelier']) .focus-box {
		display: grid;
		grid-template-columns: auto 1fr 1fr;
		gap: 8px 22px;
		align-items: center;
		text-align: left;
		margin: 14px 0;
		padding: 20px;
		border-block: 3px double var(--fg);
		background: var(--surface-hover);
	}
	:global(:root[data-appearance='atelier']) .focus-symbol {
		grid-row: span 3;
		margin: 0;
	}
	:global(:root[data-appearance='atelier']) .focus-box h3 {
		margin: 0;
	}
	:global(:root[data-appearance='atelier']) .focus-value {
		margin: 0;
	}
	:global(:root[data-appearance='atelier']) .focus-note {
		grid-column: 2 / -1;
		margin: 0;
	}
	:global(:root[data-appearance='atelier']) .focus-box::before,
	:global(:root[data-appearance='atelier']) .focus-box::after {
		display: none;
	}
	:global(:root[data-appearance='atelier']) .references {
		grid-template-columns: repeat(2, minmax(0, 1fr));
	}
	:global(:root[data-appearance='atelier']) .references li {
		border-radius: 0;
		border-width: 0 0 1px;
		padding-inline: 0;
		background: none;
	}
	:global(:root[data-appearance='aurora']) .lineage-heading {
		display: block;
		text-align: center;
	}
	:global(:root[data-appearance='aurora']) .lineage-heading > div {
		margin: auto;
	}
	:global(:root[data-appearance='aurora']) .refresh {
		margin-top: 20px;
	}
	:global(:root[data-appearance='aurora']) .lineage-spread {
		grid-template-columns: minmax(200px, 0.65fr) minmax(0, 1.7fr);
		grid-template-areas: 'focus producer' 'focus spender';
		gap: 24px;
	}
	:global(:root[data-appearance='aurora']) .branch {
		border-radius: 28px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .branch-head {
		padding: 26px;
		text-align: center;
	}
	:global(:root[data-appearance='aurora']) .focus-box {
		border-radius: 100px 100px 24px 24px;
		padding-top: 38px;
		border: var(--rule);
		margin-top: 30px;
		background: var(--surface-solid);
	}
	:global(:root[data-appearance='aurora']) .focus-box::before,
	:global(:root[data-appearance='aurora']) .focus-box::after {
		width: 16px;
	}
	@media (max-width: 1050px) {
		.lineage-spread,
		:global(:root[data-appearance='aurora']) .lineage-spread {
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas: 'producer' 'focus' 'spender';
		}
		.focus-box,
		:global(:root[data-appearance='aurora']) .focus-box {
			margin-top: 0;
		}
		.focus-box::before,
		.focus-box::after {
			display: none;
		}
	}
	@media (max-width: 650px) {
		.lineage-heading {
			display: block;
		}
		.refresh {
			margin-top: 16px;
		}
		:global(:root[data-appearance='prism']) .lineage-spread {
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas: 'producer' 'focus' 'spender';
		}
		:global(:root[data-appearance='atelier']) .branch {
			display: block;
		}
		:global(:root[data-appearance='atelier']) .branch-body {
			border: 0;
			padding-inline: 0;
		}
		:global(:root[data-appearance='atelier']) .focus-box {
			display: block;
			text-align: center;
		}
		:global(:root[data-appearance='atelier']) .focus-box > * + * {
			margin-top: 12px;
		}
		:global(:root[data-appearance='atelier']) .references {
			grid-template-columns: minmax(0, 1fr);
		}
		.branch-head,
		.branch-body {
			padding: 16px;
		}
	}
</style>
