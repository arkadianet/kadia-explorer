<script lang="ts">
	import { onMount } from 'svelte';
	import { createMempoolLoader, type MempoolState } from '$lib/mempool/observations';
	import { formatErg } from '$lib/format/amount';
	import Hash from './Hash.svelte';

	let observation = $state<MempoolState>({ loading: true, snapshot: null, error: null });
	let refresh = $state<(() => Promise<void>) | null>(null);
	onMount(() => {
		const loader = createMempoolLoader((next) => (observation = next));
		refresh = loader.refresh;
		void loader.refresh();
		return loader.stop;
	});
</script>

<section
	class="pending-preview"
	aria-labelledby="pending-preview-title"
	aria-busy={observation.loading}
>
	<div class="preview-intro">
		<h2 id="pending-preview-title">Pending transactions</h2>

		<div class="preview-actions">
			<a href="/mempool">Explore mempool <span aria-hidden="true">↗</span></a>
		</div>
	</div>
	<div class="preview-observation" aria-live="polite">
		<div class="preview-refresh">
			<button
				type="button"
				onclick={() => refresh?.()}
				disabled={observation.loading || !refresh}
				aria-label="Refresh pending preview">{observation.loading ? 'Checking…' : 'Refresh'}</button
			>
		</div>
		{#if observation.loading}
			<p class="preview-message">Checking the node’s pending transactions…</p>
		{:else if observation.error}
			<p class="preview-message">
				<strong>Observation unavailable</strong><span>{observation.error.message}</span>
			</p>
		{:else if observation.snapshot}
			<div class="preview-count">
				<strong>{observation.snapshot.observed_count.toLocaleString('en-US')}</strong>
				<span>returned by this node</span>
			</div>
			{#if observation.snapshot.items.length}
				<ul class="pending-sample" aria-label="Sample of pending transactions">
					{#each observation.snapshot.items.slice(0, 3) as tx (tx.id)}
						<li>
							<Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} />
							<span class="pending-fee"
								>{tx.fee === null ? 'Fee unavailable' : `${formatErg(tx.fee)} ERG fee`}</span
							>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="preview-message">No pending transactions returned in this observation.</p>
			{/if}
			<p class="preview-time">
				Observed <time datetime={new Date(observation.snapshot.checked_at_ms).toISOString()}
					>{new Date(observation.snapshot.checked_at_ms)
						.toISOString()
						.replace('T', ' ')
						.replace(/\.\d{3}Z$/, ' UTC')}</time
				>.
				{#if observation.snapshot.limit_reached}The 100-transaction response limit was reached.{/if}
			</p>
		{/if}
	</div>
</section>

<style>
	.pending-preview {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		grid-template-areas: 'title refresh' 'count actions' 'sample sample' 'time time';
		align-items: center;
		gap: 6px 10px;
		padding: var(--density-panel);
		margin: 0;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface);
		min-width: 0;
		width: 100%;
		box-sizing: border-box;
	}
	.preview-intro,
	.preview-observation {
		display: contents;
	}
	h2 {
		grid-area: title;
		margin: 0;
		font-size: 18px;
		line-height: 1.2;
		letter-spacing: -0.03em;
	}
	.preview-actions {
		grid-area: actions;
		font-size: 12px;
	}
	.preview-actions a {
		color: var(--accent-ink);
		font-weight: 600;
	}
	.preview-refresh {
		grid-area: refresh;
	}
	.preview-refresh button {
		min-height: 32px;
		padding: 6px 10px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--fg-muted);
		cursor: pointer;
		font: 12px var(--font-sans);
	}
	.preview-refresh button:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.preview-count {
		grid-area: count;
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 2px 6px;
	}
	.preview-count strong {
		font-size: 20px;
		line-height: 1.1;
		font-variant-numeric: tabular-nums;
	}
	.preview-count span {
		color: var(--fg-muted);
		font-size: 11px;
	}
	.pending-sample {
		grid-area: sample;
		padding: 0;
		margin: 0;
		list-style: none;
		border-top: var(--rule);
		min-width: 0;
	}
	.pending-sample li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 4px 8px;
		min-height: 36px;
		padding: 5px 0;
		border-bottom: var(--rule);
		font-size: 12px;
	}
	.pending-fee {
		font-size: 11px;
		color: var(--fg-muted);
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}
	.preview-time {
		grid-area: time;
		margin: 0;
		color: var(--fg-muted);
		font-size: 11px;
		line-height: 1.45;
	}
	.preview-message {
		grid-area: sample;
		margin: 4px 0;
		font-size: 12px;
		line-height: 1.5;
	}
	.preview-message strong,
	.preview-message span {
		display: block;
	}
	.preview-message span {
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	:global([data-appearance='prism']) .pending-preview {
		border-inline-start: 3px solid var(--accent-ink);
		background: linear-gradient(
			125deg,
			color-mix(in srgb, var(--accent) 9%, var(--surface)),
			var(--surface) 60%
		);
	}
	:global([data-appearance='prism']) .preview-count strong {
		color: var(--accent-ink);
	}
	:global([data-appearance='atelier']) .pending-preview {
		border: 0;
		border-top: 2px solid var(--fg);
		border-bottom: 1px solid var(--fg);
		border-radius: 0;
		padding-inline: 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) h2 {
		font-family: Georgia, serif;
		font-size: 20px;
		font-weight: 400;
	}
	:global([data-appearance='aurora']) .pending-preview {
		border-radius: 20px;
		background: radial-gradient(
			ellipse at 50% 0,
			color-mix(in srgb, var(--accent) 11%, var(--surface)),
			var(--surface) 70%
		);
	}
	@media (max-width: 899px), (pointer: coarse) {
		.preview-actions a,
		.preview-refresh button,
		.pending-sample :global(a) {
			min-height: 44px;
			display: inline-flex;
			align-items: center;
		}
		.pending-sample li {
			min-height: 44px;
			padding-block: 0;
		}
	}
</style>
