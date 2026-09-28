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

<section class="tracking" class:confirmed aria-label="Live transaction status">
	<div class="tracking-top">
		<p class="eyebrow">
			<span class="dot" class:stale={!!tracking.error || tracking.unsupported}></span>
			{tracking.unsupported
				? 'Live tracking unavailable'
				: tracking.error
					? 'Last known status'
					: 'Live transaction tracking'}
		</p>
		<button type="button" disabled={tracking.checking} onclick={onrefresh}
			>{tracking.checking ? 'Checking…' : 'Check now'}</button
		>
	</div>
	{#if !confirmed}
		<h2>{title}</h2>
		<p class="description">
			{#if tracking.checking && !observation}Looking for this ID in the local index and our node’s
				mempool.
			{:else if observation?.state === 'pending'}Our node has this transaction in its mempool. It
				has not been included in the local index yet.
			{:else if observation?.state === 'no_longer_observed'}Our node previously reported this
				transaction, but it is absent from its latest mempool observation. This does not establish
				that it was rejected.
			{:else if observation?.state === 'conflicted'}The local index contains another transaction
				spending a remembered input. The conflicting transactions are linked below.
			{:else if observation?.state === 'not_observed'}This ID is not in our local index or our
				node’s latest mempool observation. It may still be propagating or outside our indexed range.
				Check the ID against your wallet.
			{:else}We cannot currently check our node’s mempool. This does not mean the transaction
				failed. This page will keep checking while it is visible.{/if}
		</p>
		<div class="steps" aria-label="Transaction progress">
			<div class:reached={observation?.state === 'pending'}>
				<span>01</span><strong>Observed by our node</strong>
			</div>
			<div><span>02</span><strong>Included in a block</strong></div>
			<div><span>03</span><strong>Confirmations</strong></div>
		</div>
		{#if observation?.pending}
			<dl class="pending-facts">
				<div>
					<dt>Inputs / outputs</dt>
					<dd>{observation.pending.input_count} / {observation.pending.output_count}</dd>
				</div>
				{#if observation.pending.fee !== null}<div>
						<dt>Transaction fee</dt>
						<dd>{formatErg(observation.pending.fee)} ERG</dd>
					</div>{/if}
				{#if observation.mempool.first_seen_at_ms !== null}<div>
						<dt>First observed by this explorer</dt>
						<dd>{absTime(observation.mempool.first_seen_at_ms)}</dd>
					</div>{/if}
			</dl>
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
	<div class="observation-note">
		<p>
			{observation
				? `Checked ${absTime(observation.checked_at_ms)}.`
				: 'Awaiting a successful live observation.'}
			{tracking.unsupported ? '' : 'Checks every 5 seconds while this page is visible.'}
		</p>
		{#if observation}<details>
				<summary>What this observation covers</summary>
				<p>
					Pending status reflects one configured node. First observed means when this explorer
					observed a requested ID, not when the transaction was broadcast. Observation history is
					temporary and may be cleared early. It expires after {Math.round(
						observation.retention_seconds / 60
					)} minutes without a check and resets when the explorer restarts. Absence from one node’s mempool
					does not establish rejection. Confirmations refer to indexed blocks.
				</p>
			</details>{/if}
	</div>
</section>

<style>
	.tracking {
		min-width: 0;
		position: relative;
		overflow: hidden;
		border-radius: 20px;
		padding: 28px;
		background: #13392c;
		color: #f4fff6;
	}
	.tracking.confirmed {
		background: var(--surface-solid);
		color: var(--fg);
		border: var(--rule);
		padding: 16px 20px;
		border-radius: var(--radius-card);
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
		background: #c4f6a1;
	}
	.confirmed .dot {
		background: var(--accent-ink);
	}
	.dot.stale {
		background: #eeab71;
	}
	button {
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
		font-size: clamp(26px, 4vw, 38px);
		letter-spacing: -1px;
		line-height: 1.15;
	}
	.description {
		margin-top: 12px;
		max-width: 80ch;
		font-size: 14px;
		line-height: 1.6;
		color: #d1e5d8;
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
		border-top: 2px solid #50715f;
		padding-top: 12px;
		color: #bdcfc3;
	}
	.steps > div.reached {
		color: #c4f6a1;
		border-color: #c4f6a1;
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
		color: #bdcfc3;
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
		.tracking {
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
	}
</style>
