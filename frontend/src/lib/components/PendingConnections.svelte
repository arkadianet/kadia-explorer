<script lang="ts">
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
		onselect
	}: {
		snapshot: MempoolSnapshot;
		selected: string | null;
		invalidFocus?: boolean;
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
	<header class="connections-head">
		<div>
			<p class="eyebrow">WITHIN THIS OBSERVATION</p>
			<h2>Pending connections.</h2>
		</div>
		{#if snapshot.items.length}
			<label
				>Selected transaction
				<select
					aria-label="Select transaction connections"
					value={selected ?? ''}
					onchange={(event) => onselect(event.currentTarget.value)}
				>
					{#if !selection.transaction}<option value="" disabled>Choose a loaded transaction</option
						>{/if}
					{#each snapshot.items as item (item.id)}<option value={item.id}>{short(item.id)}</option
						>{/each}
				</select>
			</label>
		{/if}
	</header>
	{#if !context}
		<p class="connection-notice" role="status">
			This API version does not provide pending connections. The transaction list remains available.
		</p>
	{:else}
		<p class="connections-scope">
			{context.edge_count} box {context.edge_count === 1 ? 'reference' : 'references'} found among these
			{snapshot.observed_count} transactions.
			{context.identified_output_count} of {context.output_count} outputs supplied IDs. Connections cover
			this returned snapshot only.
		</p>
		{#if snapshot.limit_reached || context.identified_output_count < context.output_count || context.edges_truncated || context.shared_inputs_truncated}
			<p class="connection-notice" role="status">
				{#if snapshot.limit_reached}More transactions may exist beyond the 100 returned.{/if}
				{#if context.identified_output_count < context.output_count}Some output IDs were not
					supplied, so their connections cannot be matched.{/if}
				{#if context.edges_truncated}Showing at most {context.edges.length} of {context.edge_count} references
					across this snapshot; this transaction’s list may be incomplete.{/if}
				{#if context.shared_inputs_truncated}Shared-input details are limited; additional groups or
					transaction IDs were omitted.{/if}
			</p>
		{/if}
		{#if invalidFocus}
			<p class="connection-notice" role="status">
				The focus must be a 64-character lowercase transaction ID. Choose a transaction from this
				observation.
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
						These returned transactions reference the same spending input. This observation does not
						identify a replacement, rejection or eventual inclusion.
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
			<span class="dashed-line" aria-hidden="true"></span>Read-only data input. Unmatched inputs can
			be confirmed boxes or outside this returned page; they are not evidence of a blocked
			transaction.
		</p>
	{/if}
</section>

<style>
	.pending-connections {
		min-width: 0;
		width: 100%;
		margin-block: 0 32px;
		padding: 24px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	.connections-head {
		display: flex;
		align-items: end;
		justify-content: space-between;
		gap: 20px;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		letter-spacing: 0.1em;
		color: var(--fg-muted);
	}
	h2 {
		font: var(--weight-display, 750) 30px/1.15 var(--font-display, var(--font-sans));
		margin-top: 9px;
		letter-spacing: -0.035em;
	}
	label {
		display: grid;
		gap: 8px;
		color: var(--fg-muted);
		font-size: 11px;
		min-width: 0;
	}
	select {
		max-width: 100%;
		min-width: 0;
		padding: 11px 32px 11px 12px;
		font: 12px var(--font-mono);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
	}
	.connections-scope,
	.connection-legend,
	.connection-notice {
		font-size: 12px;
		line-height: 1.8;
		color: var(--fg-muted);
		margin-top: 18px;
	}
	.connection-notice {
		padding: 12px 16px;
		border-left: 3px solid var(--accent-ink);
		background: var(--accent-wash);
	}
	.connection-stage {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(180px, 0.85fr) minmax(0, 1fr);
		gap: 26px;
		align-items: center;
		margin-block: 26px;
	}
	.reference-side {
		min-width: 0;
		position: relative;
		align-self: stretch;
	}
	.reference-side h3 {
		font-size: 12px;
		line-height: 1.6;
		margin-bottom: 14px;
	}
	h3 span {
		color: var(--fg-muted);
		font: 11px var(--font-mono);
		margin-left: 6px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 10px;
	}
	.reference-side li {
		min-width: 0;
		padding: 13px;
		border: 1px solid var(--hairline);
		border-left: 3px solid var(--accent-ink);
		border-radius: 6px;
		position: relative;
	}
	.reference-side li.read {
		border-left-style: dashed;
	}
	.reference-side li::after {
		content: '';
		position: absolute;
		top: 50%;
		width: 26px;
		height: 0;
		border-top: 1px solid var(--accent-ink);
	}
	.producers li::after {
		right: -27px;
	}
	.consumers li::after {
		left: -27px;
	}
	.reference-side li.read::after {
		border-top-style: dashed;
	}
	.reference-kind {
		color: var(--fg-muted);
		font-size: 11px;
	}
	button {
		cursor: pointer;
		color: var(--accent-ink);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		font-size: 11px;
		padding: 9px 12px;
	}
	.neighbor {
		width: 100%;
		padding: 8px 0;
		display: flex;
		justify-content: space-between;
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
		line-height: 1.7;
	}
	.box-id {
		font: 11px var(--font-mono);
		overflow-wrap: anywhere;
	}
	details {
		margin-top: 8px;
	}
	summary {
		font-size: 11px;
		cursor: pointer;
		color: var(--accent-ink);
	}
	code {
		display: block;
		font: 11px/1.7 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
		padding-block: 6px;
	}
	.selected-node {
		min-width: 0;
		position: relative;
		text-align: center;
		padding: 24px 14px;
		background: var(--accent-wash);
		border: 1px solid var(--hairline);
		border-radius: 12px;
	}
	.node-mark {
		margin: 0 auto 18px;
		width: 52px;
		height: 52px;
		display: grid;
		place-items: center;
		border: 1px solid var(--accent-ink);
		border-radius: 12px;
		color: var(--accent-ink);
		font: 30px var(--font-display, var(--font-sans));
	}
	a {
		color: var(--accent-ink);
	}
	.selected-id {
		display: block;
		font: 11px/1.8 var(--font-mono);
		overflow-wrap: anywhere;
		word-break: break-all;
		margin-block: 12px;
	}
	.selected-node > p:not(.eyebrow) {
		font-size: 11px;
		line-height: 1.8;
	}
	.node-fee {
		margin-block: 14px;
		overflow-wrap: anywhere;
	}
	.node-fee span {
		display: block;
		color: var(--fg-muted);
	}
	.status-link {
		font-size: 11px;
		display: inline-block;
		margin-top: 8px;
	}
	.expand {
		width: 100%;
		margin-top: 12px;
	}
	.solid-line,
	.dashed-line {
		display: inline-block;
		width: 22px;
		border-top: 2px solid var(--accent-ink);
		vertical-align: middle;
		margin-right: 6px;
	}
	.dashed-line {
		border-top-style: dashed;
		margin-left: 12px;
	}
	.shared-inputs {
		padding-top: 22px;
		border-top: 1px solid var(--hairline);
	}
	.shared-inputs h3 {
		font-size: 15px;
	}
	.shared-inputs > p {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.8;
		margin-block: 10px 18px;
		max-width: 90ch;
	}
	.shared-inputs li {
		border-left: 2px solid var(--accent-ink);
		padding-left: 15px;
	}
	.shared-inputs li > span {
		font-size: 11px;
	}
	.shared-inputs li > div {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 10px;
	}
	:global([data-appearance='prism']) .pending-connections {
		background: linear-gradient(150deg, var(--surface-solid), var(--accent-wash));
		box-shadow: 0 22px 50px color-mix(in srgb, var(--accent-ink) 10%, transparent);
		border-radius: 26px;
	}
	:global([data-appearance='prism']) .connection-stage {
		gap: 40px;
		grid-template-columns: minmax(0, 1fr) minmax(200px, 0.9fr) minmax(0, 1fr);
	}
	:global([data-appearance='prism']) .reference-side li {
		background: var(--surface-solid);
		box-shadow: 0 8px 18px color-mix(in srgb, var(--fg) 5%, transparent);
		border-radius: 10px;
	}
	:global([data-appearance='prism']) .reference-side li::after {
		width: 40px;
	}
	:global([data-appearance='prism']) .producers li::after {
		right: -41px;
	}
	:global([data-appearance='prism']) .consumers li::after {
		left: -41px;
	}
	:global([data-appearance='prism']) .selected-node {
		border: 0;
		background: transparent;
		padding-block: 36px;
	}
	:global([data-appearance='prism']) .node-mark {
		width: 100px;
		height: 108px;
		font-size: 56px;
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
		box-shadow: inset 0 0 32px color-mix(in srgb, var(--accent-ink) 24%, transparent);
	}
	:global([data-appearance='atelier']) .pending-connections {
		border: 0;
		border-top: 4px double var(--fg);
		border-bottom: 1px solid var(--hairline);
		border-radius: 0;
		padding: 26px 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) .connection-stage {
		grid-template-columns: minmax(180px, 0.65fr) minmax(0, 1fr) minmax(0, 1fr);
		align-items: start;
	}
	:global([data-appearance='atelier']) .selected-node {
		grid-column: 1;
		grid-row: 1;
		text-align: left;
		padding: 0 18px 0 0;
		border: 0;
		border-right: 1px solid var(--hairline);
		border-radius: 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) .producers {
		grid-column: 2;
	}
	:global([data-appearance='atelier']) .consumers {
		grid-column: 3;
	}
	:global([data-appearance='atelier']) .node-mark {
		margin-left: 0;
		border-radius: 0;
	}
	:global([data-appearance='atelier']) .reference-side li {
		border: 0;
		border-top: 1px solid var(--hairline);
		border-left: 2px solid var(--accent-ink);
		border-radius: 0;
		padding: 12px;
	}
	:global([data-appearance='atelier']) .reference-side li.read {
		border-left-style: dashed;
	}
	:global([data-appearance='atelier']) .reference-side li::after {
		display: none;
	}
	:global([data-appearance='aurora']) .pending-connections {
		padding: 30px;
		border-radius: 30px;
	}
	:global([data-appearance='aurora']) .connections-head {
		justify-content: center;
		text-align: center;
		flex-direction: column;
		align-items: center;
	}
	:global([data-appearance='aurora']) .connections-scope {
		text-align: center;
	}
	:global([data-appearance='aurora']) .connection-stage {
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 24px;
	}
	:global([data-appearance='aurora']) .selected-node {
		grid-column: 1 / -1;
		grid-row: 1;
		width: min(100%, 540px);
		justify-self: center;
		border-radius: 26px;
		padding: 20px;
	}
	:global([data-appearance='aurora']) .node-mark {
		border-radius: 50%;
		margin-bottom: 12px;
	}
	:global([data-appearance='aurora']) .reference-side li {
		border-radius: 16px;
	}
	:global([data-appearance='aurora']) .reference-side li::after {
		display: none;
	}
	@media (max-width: 760px) {
		.pending-connections,
		:global([data-appearance='aurora']) .pending-connections {
			padding: 20px 16px;
		}
		.connections-head {
			align-items: stretch;
			flex-direction: column;
		}
		.connection-stage,
		:global([data-appearance='prism']) .connection-stage,
		:global([data-appearance='atelier']) .connection-stage,
		:global([data-appearance='aurora']) .connection-stage {
			display: flex;
			flex-direction: column;
			gap: 22px;
			align-items: stretch;
		}
		.reference-side,
		.selected-node {
			width: 100%;
		}
		.reference-side li::after {
			display: none;
		}
		:global([data-appearance='atelier']) .selected-node {
			border-right: 0;
			border-block: 1px solid var(--hairline);
			padding-block: 22px;
		}
		:global([data-appearance='aurora']) .selected-node {
			width: 100%;
		}
	}
</style>
