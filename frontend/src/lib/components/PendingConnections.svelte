<script lang="ts">
	import { onMount } from 'svelte';
	import {
		connectionsFor,
		type MempoolSnapshot,
		type PendingConnection
	} from '$lib/mempool/observations';
	import { formatErg } from '$lib/format/amount';
	let {
		snapshot,
		selected,
		invalidFocus = false,
		focused = false,
		open = $bindable(false),
		onselect
	}: {
		snapshot: MempoolSnapshot;
		selected: string | null;
		invalidFocus?: boolean;
		focused?: boolean;
		open?: boolean;
		onselect: (id: string) => void;
	} = $props();
	const context = $derived(snapshot.connections);
	const selection = $derived(connectionsFor(snapshot, selected ?? ''));
	let expanded = $state({ incoming: false, outgoing: false });
	$effect(() => {
		// Expansion belongs to the selected observation, never the next refreshed graph.
		void selected;
		void snapshot;
		expanded = { incoming: false, outgoing: false };
	});
	const short = (id: string) => `${id.slice(0, 8)}…${id.slice(-8)}`;
	onMount(() => {
		const desktop = window.matchMedia('(min-width: 1000px)');
		open = desktop.matches || focused;
		const resize = () => {
			open = desktop.matches;
		};
		desktop.addEventListener('change', resize);
		return () => desktop.removeEventListener('change', resize);
	});
</script>

{#snippet reference(edge: PendingConnection, incoming: boolean)}
	{@const neighbor = incoming ? edge.producer_id : edge.consumer_id}
	<li class:read={edge.kind === 'read'}>
		<span class="reference-kind"
			>{edge.kind === 'read' ? 'Read-only reference' : 'Spending input'}</span
		>
		<button
			type="button"
			class="neighbor"
			onclick={() => onselect(neighbor)}
			aria-label={'Inspect connections for transaction ' + neighbor}
		>
			<span>{short(neighbor)}</span><span aria-hidden="true">↗</span>
		</button>
		<p>
			{incoming ? 'Produces' : edge.kind === 'read' ? 'Reads' : 'Spends'} box
			<span class="box-id" title={edge.box_id}>{short(edge.box_id)}</span>
		</p>
		<details><summary>Box reference</summary><code>{edge.box_id}</code></details>
	</li>
{/snippet}

<section class="pending-connections" aria-label="Pending connections">
	<details class="inspector" bind:open>
		<summary class="inspector-toggle"
			><span>Pending connections</span><small
				>{selection.transaction ? short(selection.transaction.id) : 'Snapshot inspector'}</small
			></summary
		>
		<!-- The bounded desktop scroll region needs keyboard focus for scrolling its evidence. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="inspector-body"
			tabindex="0"
			role="region"
			aria-label="Connection inspector contents"
		>
			<header class="connections-head">
				{#if snapshot.items.length}
					<label
						>Selected transaction
						<select
							aria-label="Select transaction connections"
							value={selected ?? ''}
							onchange={(event) => onselect(event.currentTarget.value)}
						>
							{#if !selection.transaction}<option value="" disabled
									>Choose a loaded transaction</option
								>{/if}
							{#each snapshot.items as item (item.id)}<option value={item.id}
									>{short(item.id)}</option
								>{/each}
						</select>
					</label>
				{/if}
			</header>
			{#if !context}
				<p class="connection-notice" role="status">
					This API version does not provide pending connections. The transaction list remains
					available.
				</p>
			{:else}
				<p class="connections-scope">
					{context.edge_count} box {context.edge_count === 1 ? 'reference' : 'references'} found among
					these
					{snapshot.observed_count} transactions.
					{context.identified_output_count} of {context.output_count} outputs supplied IDs. Connections
					cover this returned snapshot only.
				</p>
				{#if snapshot.limit_reached || context.identified_output_count < context.output_count || context.edges_truncated || context.shared_inputs_truncated}
					<p class="connection-notice" role="status">
						{#if snapshot.limit_reached}More transactions may exist beyond the 100 returned.{/if}
						{#if context.identified_output_count < context.output_count}Some output IDs were not
							supplied, so their connections cannot be matched.{/if}
						{#if context.edges_truncated}Showing at most {context.edges.length} of {context.edge_count}
							references across this snapshot; this transaction’s list may be incomplete.{/if}
						{#if context.shared_inputs_truncated}Shared-input details are limited; additional groups
							or transaction IDs were omitted.{/if}
					</p>
				{/if}
				{#if invalidFocus}
					<p class="connection-notice" role="status">
						The focus must be a 64-character lowercase transaction ID. Choose a transaction from
						this observation.
					</p>
				{:else if selected && !selection.transaction}
					<p class="connection-notice" role="status">
						The requested transaction is not in this loaded snapshot. This does not establish its
						current status. <a href={'/tx/' + selected}>Check transaction status ↗</a>
					</p>
				{:else if selection.transaction}
					<div class="connection-stage">
						<div class="reference-side producers">
							<h3>Produces referenced boxes <span>{selection.incoming.length}</span></h3>
							{#if selection.incoming.length}<ul aria-label="Observed producers">
									{#each selection.incoming.slice(0, expanded.incoming ? undefined : 4) as edge (`${edge.producer_id}:${edge.box_id}:${edge.kind}`)}{@render reference(
											edge,
											true
										)}{/each}
								</ul>{:else}<p class="no-reference">
									No producer reference is included for this transaction.
								</p>{/if}
							{#if selection.incoming.length > 4}<button
									class="expand"
									type="button"
									onclick={() => (expanded = { ...expanded, incoming: !expanded.incoming })}
									>{expanded.incoming
										? 'Show fewer producer references'
										: `Show all ${selection.incoming.length} producer references`}</button
								>{/if}
						</div>
						<div class="selected-node" aria-label="Selected pending transaction">
							<div class="node-mark" aria-hidden="true">Σ</div>
							<p class="eyebrow">SELECTED TRANSACTION</p>
							<a class="selected-id" href={'/tx/' + selection.transaction.id}
								>{selection.transaction.id}</a
							>
							<p>
								{selection.transaction.input_count} inputs · {selection.transaction.output_count} outputs
							</p>
							{#if selection.transaction.fee !== null}<p class="node-fee">
									{formatErg(selection.transaction.fee)} ERG<span>Miner-fee outputs</span>
								</p>{/if}
							<a class="status-link" href={'/tx/' + selection.transaction.id}
								>Check transaction status ↗</a
							>
						</div>
						<div class="reference-side consumers">
							<h3>References these outputs <span>{selection.outgoing.length}</span></h3>
							{#if selection.outgoing.length}<ul aria-label="Observed consumers">
									{#each selection.outgoing.slice(0, expanded.outgoing ? undefined : 4) as edge (`${edge.consumer_id}:${edge.box_id}:${edge.kind}`)}{@render reference(
											edge,
											false
										)}{/each}
								</ul>{:else}<p class="no-reference">
									No consumer reference is included for this transaction.
								</p>{/if}
							{#if selection.outgoing.length > 4}<button
									class="expand"
									type="button"
									onclick={() => (expanded = { ...expanded, outgoing: !expanded.outgoing })}
									>{expanded.outgoing
										? 'Show fewer consumer references'
										: `Show all ${selection.outgoing.length} consumer references`}</button
								>{/if}
						</div>
					</div>
					{#if selection.shared.length}
						<div class="shared-inputs" aria-label="Shared spending inputs">
							<h3>Shared spending inputs observed</h3>
							<p>
								These returned transactions reference the same spending input. This observation does
								not identify a replacement, rejection or eventual inclusion.
							</p>
							<ul>
								{#each selection.shared as group (group.box_id)}<li>
										<span>Box <code>{group.box_id}</code></span>
										<p>
											{group.transaction_count} transactions reference it{group.truncated
												? `; ${group.transaction_ids.length} IDs returned`
												: ''}.
										</p>
										<div>
											{#each group.transaction_ids as id (id)}<button
													type="button"
													onclick={() => onselect(id)}
													aria-label={'Inspect shared-input transaction ' + id}>{short(id)}</button
												>{/each}
										</div>
									</li>{/each}
							</ul>
						</div>
					{/if}
				{/if}
				<p class="connection-legend">
					<span class="solid-line" aria-hidden="true"></span>Spending input
					<span class="dashed-line" aria-hidden="true"></span>Read-only data input. Unmatched inputs
					can be confirmed boxes or outside this returned page; they are not evidence of a blocked
					transaction.
				</p>
			{/if}
		</div>
	</details>
</section>

<style>
	.pending-connections {
		min-width: 0;
		width: 100%;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	.inspector-toggle {
		padding: 10px var(--density-panel, 16px);
		min-height: 44px;
		color: var(--fg);
		font: var(--weight-display, 700) 17px/1.5 var(--font-display, var(--font-sans));
	}
	.inspector-toggle small {
		display: block;
		padding-left: 18px;
		font: 11px/1.6 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.inspector-body {
		min-width: 0;
		padding: 0 var(--density-panel, 16px) var(--density-panel, 16px);
	}
	.connections-head {
		min-width: 0;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		letter-spacing: 0.06em;
		color: var(--fg-muted);
	}
	label {
		display: grid;
		gap: 5px;
		color: var(--fg-muted);
		font-size: 11px;
		min-width: 0;
	}
	select {
		width: 100%;
		min-width: 0;
		min-height: 36px;
		padding: 7px 24px 7px 9px;
		font: 11px var(--font-mono);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
	}
	.connections-scope,
	.connection-legend,
	.connection-notice {
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
		margin-top: 10px;
	}
	.connection-notice {
		padding: 8px 10px;
		border-left: 2px solid var(--accent-ink);
		background: var(--accent-wash);
	}
	.connection-stage {
		display: flex;
		flex-direction: column;
		gap: var(--density-gap, 16px);
		margin-block: var(--density-gap, 16px);
	}
	.reference-side {
		min-width: 0;
		position: relative;
	}
	.reference-side h3 {
		font-size: 12px;
		line-height: 1.6;
		margin-bottom: 8px;
	}
	h3 span {
		color: var(--fg-muted);
		font: 11px var(--font-mono);
		margin-left: 5px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 8px;
	}
	.reference-side li {
		min-width: 0;
		padding: 7px 10px;
		border: 1px solid var(--hairline);
		border-left: 2px solid var(--accent-ink);
		border-radius: 5px;
	}
	.reference-side li.read {
		border-left-style: dashed;
	}
	.reference-kind {
		color: var(--fg-muted);
		font-size: 11px;
	}
	button {
		cursor: pointer;
		min-height: 32px;
		color: var(--accent-ink);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		font-size: 11px;
		padding: 6px 9px;
	}
	.neighbor {
		width: 100%;
		padding: 5px 0;
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 5px;
		border: 0;
		background: transparent;
		font: 11px var(--font-mono);
		text-align: left;
	}
	.neighbor span:first-child {
		overflow-wrap: anywhere;
	}
	li p,
	.no-reference {
		color: var(--fg-muted);
		font-size: 11px;
		line-height: 1.6;
	}
	.box-id {
		font: 11px var(--font-mono);
		overflow-wrap: anywhere;
	}
	summary {
		min-height: 32px;
		align-content: center;
		font-size: 11px;
		cursor: pointer;
		color: var(--accent-ink);
	}
	code {
		display: block;
		font: 11px/1.7 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
		padding-block: 5px;
	}
	.selected-node {
		min-width: 0;
		position: relative;
		padding: 10px;
		background: var(--accent-wash);
		border: 1px solid var(--hairline);
		border-radius: 8px;
	}
	.node-mark {
		float: left;
		margin: 0 9px 4px 0;
		width: 32px;
		height: 32px;
		display: grid;
		place-items: center;
		border: 1px solid var(--accent-ink);
		border-radius: 8px;
		color: var(--accent-ink);
		font: 22px var(--font-display, var(--font-sans));
	}
	a {
		color: var(--accent-ink);
	}
	.selected-id {
		display: block;
		font: 11px/1.6 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
		margin-block: 6px;
	}
	.selected-node > p:not(.eyebrow) {
		font-size: 11px;
		line-height: 1.6;
	}
	.node-fee {
		margin-block: 6px;
		overflow-wrap: anywhere;
	}
	.node-fee span {
		display: block;
		color: var(--fg-muted);
	}
	.status-link {
		font-size: 11px;
		display: inline-flex;
		align-items: center;
		min-height: 32px;
	}
	.expand {
		width: 100%;
		margin-top: 8px;
	}
	.solid-line,
	.dashed-line {
		display: inline-block;
		width: 16px;
		border-top: 2px solid var(--accent-ink);
		vertical-align: middle;
		margin-right: 4px;
	}
	.dashed-line {
		border-top-style: dashed;
		margin-left: 8px;
	}
	.shared-inputs {
		padding-top: 10px;
		border-top: 1px solid var(--hairline);
	}
	.shared-inputs h3 {
		font-size: 13px;
	}
	.shared-inputs > p {
		color: var(--fg-muted);
		font-size: 11px;
		line-height: 1.6;
		margin-block: 8px;
	}
	.shared-inputs li {
		border-left: 2px solid var(--accent-ink);
		padding-left: 10px;
	}
	.shared-inputs li > span {
		font-size: 11px;
	}
	.shared-inputs li > div {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 8px;
	}
	:global([data-appearance='prism']) .pending-connections {
		background: linear-gradient(150deg, var(--surface-solid), var(--accent-wash));
		box-shadow: 0 10px 24px color-mix(in srgb, var(--accent-ink) 8%, transparent);
		border-radius: 16px;
	}
	:global([data-appearance='prism']) .reference-side {
		padding-left: 12px;
		border-left: 1px solid var(--accent-ink);
	}
	:global([data-appearance='prism']) .reference-side li {
		background: var(--surface-solid);
		border-radius: 8px;
	}
	:global([data-appearance='prism']) .selected-node {
		border: 0;
		background: transparent;
	}
	:global([data-appearance='prism']) .node-mark {
		width: 40px;
		height: 44px;
		font-size: 28px;
		border: 0;
		border-radius: 0;
		clip-path: polygon(50% 0, 100% 28%, 91% 80%, 50% 100%, 9% 80%, 0 28%);
		background: conic-gradient(
			from 25deg at 45% 45%,
			var(--surface-solid),
			var(--accent-wash),
			var(--surface-solid),
			color-mix(in srgb, var(--accent-ink) 35%, var(--surface-solid)),
			var(--surface-solid)
		);
		box-shadow: inset 0 0 15px color-mix(in srgb, var(--accent-ink) 24%, transparent);
	}
	:global([data-appearance='atelier']) .pending-connections {
		border: 0;
		border-top: 4px double var(--fg);
		border-radius: 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) .inspector-toggle,
	:global([data-appearance='atelier']) .inspector-body {
		padding-inline: 0;
	}
	:global([data-appearance='atelier']) .selected-node {
		order: -1;
		border: 0;
		border-bottom: 1px solid var(--hairline);
		border-radius: 0;
		padding: 0 0 10px;
		background: transparent;
	}
	:global([data-appearance='atelier']) .node-mark {
		border-radius: 0;
	}
	:global([data-appearance='atelier']) .reference-side li {
		border: 0;
		border-left: 2px solid var(--accent-ink);
		border-radius: 0;
		padding-block: 0;
	}
	:global([data-appearance='atelier']) .reference-side li.read {
		border-left-style: dashed;
	}
	:global([data-appearance='aurora']) .pending-connections {
		border-radius: 22px;
	}
	:global([data-appearance='aurora']) .selected-node {
		order: -1;
		border-radius: 18px;
		text-align: center;
	}
	:global([data-appearance='aurora']) .node-mark {
		float: none;
		margin: 0 auto 6px;
		border-radius: 50%;
	}
	:global([data-appearance='aurora']) .reference-side {
		border-radius: 14px;
		padding: 10px;
		background: var(--accent-wash);
	}
	:global([data-appearance='aurora']) .reference-side li {
		border-radius: 10px;
		background: var(--surface-solid);
	}
	@media (min-width: 1000px) {
		.inspector-body {
			max-height: calc(100dvh - 150px);
			overflow-y: auto;
			scrollbar-width: thin;
			overscroll-behavior-y: contain;
		}
	}
	@media (max-width: 999px) {
		button,
		select,
		summary,
		.status-link,
		.selected-id {
			min-height: 44px;
		}
		.selected-id {
			align-content: center;
		}
	}
</style>
