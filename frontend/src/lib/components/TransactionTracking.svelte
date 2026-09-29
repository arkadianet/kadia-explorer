<script lang="ts">
	import type { TrackingState } from '$lib/tx/tracker';
	import { absTime } from '$lib/format/time';
	import { formatErg } from '$lib/format/amount';
	import Hash from './Hash.svelte';
	let { tracking, onrefresh }: { tracking: TrackingState; onrefresh: () => void } = $props();
	const observation = $derived(tracking.status);
	const confirmed = $derived(
		observation?.state === 'confirmed' || (tracking.tx !== null && observation === null)
	);
	const hasPendingDetails = $derived(!!observation?.pending?.details && !confirmed);
	const hasConfirmedReceipt = $derived(confirmed && tracking.tx !== null);
	const contextId = $derived(observation?.id ?? tracking.tx?.id ?? '');
	const title = $derived(
		observation?.state === 'pending'
			? 'Waiting for a block'
			: observation?.state === 'no_longer_observed'
				? 'No longer in our node’s mempool'
				: observation?.state === 'conflicted'
					? 'An input was spent by another transaction'
					: confirmed
						? 'Included in a block'
						: observation?.state === 'not_observed'
							? observation.previous_inclusion
								? 'Previous inclusion changed'
								: 'Not observed yet'
							: tracking.checking && !observation
								? 'Checking this transaction'
								: 'Live status unavailable'
	);
</script>

{#snippet trackingLabel()}
	<p class="eyebrow">
		<span class="dot" class:stale={!!tracking.error || tracking.unsupported}></span>
		{tracking.unsupported
			? 'Live tracking unavailable'
			: tracking.error
				? 'Last known status'
				: 'Live transaction tracking'}
	</p>
{/snippet}

{#snippet checkedAt()}
	<p>
		{observation
			? `Checked ${absTime(observation.checked_at_ms)}.`
			: 'Awaiting a successful live observation.'}
		{tracking.unsupported ? '' : 'Checks every 5 seconds while this page is visible.'}
	</p>
{/snippet}

{#snippet progress()}
	<div class="steps" aria-label="Transaction progress">
		<div class:reached={observation?.state === 'pending'}>
			<span>01</span><strong>Observed by our node</strong>
		</div>
		<div><span>02</span><strong>Included in a block</strong></div>
		<div><span>03</span><strong>Confirmations</strong></div>
	</div>
{/snippet}

{#snippet secondaryPendingFacts()}
	{#if observation?.pending}
		<div>
			<dt>Read-only data inputs</dt>
			<dd>{observation.pending.data_input_count}</dd>
		</div>
		<div>
			<dt>Serialized size</dt>
			<dd>
				{observation.pending.size === null
					? 'Not supplied'
					: `${observation.pending.size.toLocaleString('en-US')} bytes`}
			</dd>
		</div>
		{#if observation.mempool.first_seen_at_ms !== null}<div>
				<dt>First observed by this explorer</dt>
				<dd>{absTime(observation.mempool.first_seen_at_ms)}</dd>
			</div>{/if}
	{/if}
{/snippet}

{#snippet statusDescription()}
	<p class="description">
		{#if tracking.checking && !observation}Looking for this ID in the local index and our node’s
			mempool.
		{:else if observation?.state === 'pending'}Our node has this transaction in its mempool. It has
			not been included in the local index yet.
		{:else if observation?.state === 'no_longer_observed'}Our node previously reported this
			transaction, but it is absent from its latest mempool observation. This does not establish
			that it was rejected.
		{:else if observation?.state === 'conflicted'}The local index contains another transaction
			spending a remembered input. The conflicting transactions are linked below.
		{:else if observation?.state === 'not_observed'}This ID is not in our local index or our node’s
			latest mempool observation. It may still be propagating or outside our indexed range. Check
			the ID against your wallet.
		{:else}We cannot currently check our node’s mempool. This does not mean the transaction failed.
			This page will keep checking while it is visible.{/if}
	</p>
{/snippet}

{#snippet connectionLink()}
	{#if observation?.pending}
		<p class="connections-link">
			<a
				href={'/mempool?focus=' + observation.id}
				aria-label="Inspect connections in a mempool snapshot"
				>{hasPendingDetails ? 'Connections ↗' : 'Inspect connections in a mempool snapshot ↗'}</a
			>
		</p>
	{/if}
{/snippet}

{#snippet observationNote()}
	<div class="observation-note">
		{#if !confirmed && !hasPendingDetails}{@render checkedAt()}{/if}
		{#if observation || hasConfirmedReceipt}<details class:progress-details={hasPendingDetails}>
				<summary
					>{hasPendingDetails
						? 'Observation details'
						: confirmed
							? 'Observation details'
							: 'What this observation covers'}</summary
				>
				{#if confirmed || hasPendingDetails}{@render checkedAt()}{/if}{#if hasPendingDetails || hasConfirmedReceipt}<p
						class="full-transaction-id"
					>
						<strong>Transaction ID</strong><code>{contextId}</code>
					</p>{/if}
				{#if hasPendingDetails}
					{@render statusDescription()}{@render progress()}
					<dl class="pending-facts">{@render secondaryPendingFacts()}</dl>{/if}
				{#if observation}<p>
						Pending status reflects one configured node. First observed means when this explorer
						observed a requested ID, not when the transaction was broadcast. Observation history is
						temporary and may be cleared early. It expires after {Math.round(
							observation.retention_seconds / 60
						)} minutes without a check and resets when the explorer restarts. Absence from one node’s
						mempool does not establish rejection. Confirmations refer to indexed blocks.
					</p>{/if}
			</details>{/if}
	</div>
{/snippet}

<section
	class={hasPendingDetails
		? 'pending-context'
		: hasConfirmedReceipt
			? 'confirmed-context'
			: 'tracking'}
	class:confirmed={confirmed && !hasConfirmedReceipt}
	class:has-pending-details={hasPendingDetails}
	aria-label="Live transaction status"
>
	{#if hasPendingDetails || hasConfirmedReceipt}
		<header class="pending-context-head">
			<div class="pending-identity">
				<h1>Transaction</h1>
				<Hash value={contextId} head={6} tail={4} copyLabel="Copy transaction ID" />
			</div>
			<button type="button" disabled={tracking.checking} onclick={onrefresh}
				>{tracking.checking ? 'Checking…' : 'Check now'}</button
			>
		</header>
	{:else}
		<div class="tracking-top">
			{@render trackingLabel()}
			<button type="button" disabled={tracking.checking} onclick={onrefresh}
				>{tracking.checking ? 'Checking…' : 'Check now'}</button
			>
		</div>
	{/if}
	{#if !confirmed}
		<h2 aria-live="polite" aria-atomic="true">{title}</h2>
		{#if !hasPendingDetails}{@render statusDescription()}{/if}
		{#if !hasPendingDetails}{@render progress()}{/if}
		{#if observation?.pending}
			{#if observation.mempool.observation !== 'present'}<p class="warning">
					The pending facts below are from the last time this explorer observed the transaction{observation
						.mempool.last_seen_at_ms !== null
						? ` (${absTime(observation.mempool.last_seen_at_ms)})`
						: ''}; they do not establish its current status.
				</p>{/if}
			<dl class="pending-facts">
				<div>
					<dt>Inputs / outputs</dt>
					<dd>{observation.pending.input_count} / {observation.pending.output_count}</dd>
				</div>
				{#if observation.pending.fee !== null}<div>
						<dt>Miner-fee outputs</dt>
						<dd title={observation.pending.fee + ' nanoERG'}>
							{formatErg(observation.pending.fee)} ERG
						</dd>
					</div>{/if}
				{#if !observation.pending.details}{@render secondaryPendingFacts()}{/if}
			</dl>
			{#if !hasPendingDetails}{@render connectionLink()}{/if}
		{/if}
	{:else if !tracking.tx}<p class="description">
			Included at block {observation?.inclusion?.height.toLocaleString('en-US')}. Loading the
			receipt for this block…
		</p>{/if}
	{#if tracking.error}<p class="warning" role="status">{tracking.error}</p>{/if}
	{#if tracking.unsupported}<p class="warning">
			This server does not provide live transaction status. {tracking.tx
				? 'The receipt is a saved confirmation snapshot.'
				: 'No transaction observation is available.'}
		</p>{/if}
	{#if observation?.previous_inclusion}
		<p class="warning">
			The previous inclusion at block {observation.previous_inclusion.height.toLocaleString(
				'en-US'
			)} is no longer in the local index. The index may be catching up or its chain may have changed.
		</p>
	{/if}
	{#if observation?.conflicts.length}
		<ul class="conflicts">
			{#each observation.conflicts as conflict (conflict.input_id)}
				<li>
					Input <Hash value={conflict.input_id} href={`/box/${conflict.input_id}`} copy={false} /> was
					spent by <Hash value={conflict.tx_id} href={`/tx/${conflict.tx_id}`} copy={false} /> at block
					{conflict.height.toLocaleString('en-US')}.
				</li>
			{/each}
		</ul>
	{/if}
	{#if hasConfirmedReceipt}<div class="confirmed-status-row">
			{@render trackingLabel()}{@render observationNote()}
		</div>{:else if hasPendingDetails}<div class="pending-actions">
			{@render connectionLink()}{@render observationNote()}
		</div>{:else}{@render observationNote()}{/if}
</section>

<style>
	.confirmed-context {
		display: grid;
		gap: 0;
		min-width: 0;
		padding: 8px 0;
		border-block: 1px solid var(--hairline);
		color: var(--fg);
	}
	.confirmed-context .dot {
		background: var(--accent-ink);
	}
	.confirmed-context .dot.stale {
		background: #eeab71;
	}
	.confirmed-status-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 0 12px;
		min-width: 0;
	}
	.confirmed-status-row .eyebrow {
		font-size: 10px;
		letter-spacing: 0.03em;
		color: var(--fg-muted);
	}
	.confirmed-status-row .observation-note,
	.confirmed-status-row details {
		margin: 0;
		opacity: 1;
	}
	.confirmed-status-row:has(details[open]) {
		grid-template-columns: minmax(0, 1fr);
	}
	.confirmed-context > .warning,
	.confirmed-context > .conflicts {
		margin-block: 8px;
	}
	:global([data-appearance='prism']) .confirmed-context {
		border-inline-start: 3px solid var(--accent-ink);
		padding-inline: 12px;
		background: linear-gradient(110deg, var(--surface-solid), var(--accent-wash));
	}
	:global([data-appearance='atelier']) .confirmed-context {
		border-top: 3px double var(--fg);
	}
	:global([data-appearance='aurora']) .confirmed-context {
		padding-inline: 12px;
		border: 1px solid var(--hairline);
		border-radius: 18px;
		background: var(--surface-solid);
	}
	@media (min-width: 900px) {
		.confirmed-context {
			grid-template-columns: minmax(0, 1fr) auto;
			gap: 0 24px;
		}
		.confirmed-context .pending-context-head {
			grid-column: 1;
		}
		.confirmed-context > .warning,
		.confirmed-context > .conflicts {
			grid-column: 1 / -1;
		}
		.confirmed-status-row {
			grid-column: 2;
			grid-row: 1;
			gap: 0 20px;
		}
		.confirmed-status-row:has(details[open]) {
			grid-column: 1 / -1;
			grid-row: auto;
		}
		:global([data-appearance='atelier']) .confirmed-context {
			grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr);
			column-gap: 32px;
		}
		:global([data-appearance='aurora']) .confirmed-context {
			padding-inline: 20px;
		}
	}
	.pending-context {
		--hero-muted: var(--fg-muted);
		--hero-highlight: var(--accent-ink);
		--hero-hairline: var(--hairline);
		--tracking-link: var(--accent-ink);
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 8px 18px;
		min-width: 0;
		color: var(--fg);
		padding: 12px 0;
		border-block: 1px solid var(--hairline);
	}
	.pending-context-head {
		grid-column: 1 / -1;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-width: 0;
	}
	.pending-identity {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0 8px;
		min-width: 0;
	}
	:global(:root[data-density][data-appearance] .content[data-page='tx']) .pending-context h1,
	:global(:root[data-density][data-appearance] .content[data-page='tx']) .confirmed-context h1 {
		font: var(--weight-display, 750) 22px/1.15 var(--font-display, var(--font-sans));
		letter-spacing: -0.03em;
		margin: 0;
	}
	.pending-identity :global(.hash) {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	.pending-identity :global(.copy) {
		min-width: 44px;
		min-height: 44px;
	}
	.pending-context > h2 {
		margin: 0;
		font-size: 18px;
		line-height: 1.25;
		letter-spacing: -0.02em;
		align-self: center;
	}
	.pending-context > .pending-facts {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 8px 20px;
		align-items: center;
	}
	.pending-context dt {
		font-size: 11px;
	}
	.pending-context dd {
		margin-top: 2px;
		font-size: 13px;
	}
	.pending-actions {
		grid-column: 1 / -1;
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: start;
		gap: 8px 20px;
	}
	.pending-actions .connections-link {
		margin: 0;
	}
	.pending-actions .connections-link a {
		min-height: 44px;
		display: inline-flex;
		align-items: center;
	}
	.pending-actions .observation-note,
	.pending-actions details {
		margin: 0;
		opacity: 1;
	}
	.pending-actions summary {
		width: fit-content;
		margin-left: auto;
		font-size: 12px;
	}
	.pending-actions:has(details[open]) {
		grid-template-columns: 1fr;
	}
	.pending-actions:has(details[open]) summary {
		margin-left: 0;
	}
	.pending-context > :is(.warning, .conflicts) {
		grid-column: 1 / -1;
		margin-top: 0;
	}
	.pending-context .description {
		color: var(--fg-muted);
		font-size: 12px;
	}
	.full-transaction-id code {
		display: block;
		font: 11px/1.7 var(--font-mono);
		overflow-wrap: anywhere;
	}
	.full-transaction-id strong {
		font-size: 11px;
	}
	:global([data-appearance='prism']) .pending-context {
		border: 1px solid var(--hairline);
		border-radius: 14px;
		padding: 10px 14px;
		background: linear-gradient(135deg, var(--surface-solid), var(--accent-wash));
	}
	:global([data-appearance='atelier']) .pending-context {
		border-top: 4px double var(--fg);
		border-bottom: 1px solid var(--hairline);
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .pending-context {
		border: 1px solid var(--hairline);
		border-radius: 22px;
		padding: 10px 16px;
		background: var(--surface-solid);
	}
	:global([data-appearance='aurora']) .pending-context > .pending-facts {
		justify-content: end;
	}
	@media (min-width: 900px) {
		:global([data-appearance='atelier']) .pending-context {
			grid-template-columns: minmax(190px, 0.4fr) minmax(0, 1fr);
			column-gap: 24px;
		}
		:global([data-appearance='atelier']) .pending-context-head {
			grid-column: 1;
			grid-row: 1 / 3;
			display: grid;
			align-content: start;
			justify-content: start;
			border-right: 1px solid var(--hairline);
			padding-right: 18px;
		}
		:global([data-appearance='atelier']) .pending-context-head > button {
			justify-self: start;
		}
		:global([data-appearance='atelier']) .pending-context > h2,
		:global([data-appearance='atelier']) .pending-context > .pending-facts,
		:global([data-appearance='atelier']) .pending-actions {
			grid-column: 2;
		}
		:global([data-appearance='aurora']) .pending-context-head {
			justify-content: center;
			gap: 24px;
		}
	}
	@media (max-width: 700px) {
		.pending-context {
			grid-template-columns: minmax(0, 1fr);
			gap: 6px;
			padding-block: 8px;
		}
		.pending-context > .pending-facts {
			gap: 8px 22px;
		}
		:global([data-appearance='aurora']) .pending-context > .pending-facts {
			justify-content: start;
		}
		.pending-context .pending-actions {
			column-gap: 12px;
		}
	}
	.connections-link {
		margin-block: 16px;
		font-size: 12px;
		line-height: 1.7;
	}
	.connections-link a {
		color: var(--tracking-link, var(--accent-ink));
	}
	.tracking {
		min-width: 0;
		position: relative;
		overflow: hidden;
		border-radius: 20px;
		padding: 28px;
		background: var(--hero-bg, #103a28);
		border-top: 3px solid var(--hero-highlight, #b7f25f);
		color: var(--hero-fg, #f4fff6);
	}
	.tracking.confirmed {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto auto;
		gap: 0 20px;
		background: var(--surface-solid);
		color: var(--fg);
		border: var(--rule);
		padding: 12px 20px;
		border-radius: var(--radius-card);
	}
	.tracking:not(.confirmed) {
		--accent-ink: var(--tracking-link, #b7f25f);
	}
	.confirmed .tracking-top {
		display: contents;
	}
	.confirmed .eyebrow {
		grid-column: 1;
		grid-row: 1;
		align-self: center;
	}
	.confirmed button {
		grid-column: 3;
		grid-row: 1;
	}
	.confirmed .observation-note {
		grid-column: 2;
		grid-row: 1;
		align-self: center;
		margin-top: 0;
	}
	.confirmed details {
		margin-top: 0;
	}
	.confirmed .observation-note:has(details[open]) {
		grid-column: 1 / -1;
		grid-row: auto;
		margin-top: 10px;
	}
	.confirmed .warning,
	.confirmed .description,
	.confirmed .conflicts {
		grid-column: 1 / -1;
	}
	.tracking-top {
		display: flex;
		gap: 16px;
		justify-content: space-between;
		align-items: center;
	}
	.eyebrow {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 11px;
		letter-spacing: 0.07em;
		text-transform: uppercase;
	}
	.dot {
		height: 7px;
		width: 7px;
		flex: none;
		border-radius: 50%;
		background: var(--hero-highlight, #c4f6a1);
	}
	.confirmed .dot {
		background: var(--accent-ink);
	}
	.dot.stale {
		background: #eeab71;
	}
	button {
		min-height: 44px;
		flex: none;
		background: transparent;
		color: inherit;
		border: 1px solid currentColor;
		border-radius: var(--radius-pill);
		padding: 7px 12px;
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.6;
		cursor: wait;
	}
	h2 {
		margin-top: 24px;
		font-size: clamp(28px, 4vw, 42px);
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 800);
		letter-spacing: -1px;
		line-height: 1.15;
	}
	.description {
		margin-top: 12px;
		max-width: 80ch;
		font-size: 14px;
		line-height: 1.6;
		color: var(--hero-muted, #d1e5d8);
	}
	.confirmed .description {
		color: var(--fg-muted);
	}
	.steps {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 20px;
		margin-top: 28px;
	}
	.steps > div {
		border-top: 2px solid var(--hero-hairline, #50715f);
		padding-top: 12px;
		color: var(--hero-muted, #bdcfc3);
	}
	.steps > div.reached {
		color: var(--hero-highlight, #c4f6a1);
		border-color: var(--hero-highlight, #c4f6a1);
	}
	.steps span {
		font-size: 11px;
		display: block;
		margin-bottom: 8px;
	}
	.steps strong {
		font-size: 13px;
		font-weight: 500;
	}
	.pending-facts {
		display: flex;
		flex-wrap: wrap;
		gap: 20px 36px;
		margin-top: 28px;
	}
	dt {
		font-size: 12px;
		color: var(--hero-muted, #bdcfc3);
	}
	dd {
		margin: 6px 0 0;
		font-size: 14px;
	}
	.warning {
		margin-top: 16px;
		padding: 12px 16px;
		background: #ffe3b71a;
		border-left: 2px solid #d99c6f;
		font-size: 13px;
	}
	.observation-note {
		margin-top: 16px;
		font-size: 12px;
		line-height: 1.6;
		opacity: 0.8;
	}
	details {
		margin-top: 8px;
	}
	summary {
		cursor: pointer;
		min-height: 44px;
		align-content: center;
	}
	.progress-details {
		margin-top: 12px;
		font-size: 12px;
	}
	.progress-details .steps {
		margin-top: 8px;
	}
	details p {
		max-width: 85ch;
		margin-top: 8px;
	}
	.conflicts {
		margin-top: 16px;
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.conflicts li + li {
		margin-top: 8px;
	}
	@media (max-width: 500px) {
		:global(:root[data-appearance='atelier'] .content[data-page='tx']) .tracking {
			padding: 20px;
		}
		.steps {
			gap: 12px;
		}
		.tracking-top {
			gap: 8px;
		}
		.eyebrow {
			font-size: 10px;
		}
		.confirmed .observation-note {
			grid-column: 1 / -1;
			grid-row: auto;
			margin-top: 8px;
		}
	}
</style>
