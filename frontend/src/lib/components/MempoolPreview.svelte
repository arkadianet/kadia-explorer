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
		<p class="eyebrow">Before the next block</p>
		<h2 id="pending-preview-title">Pending transactions</h2>
		<p class="scope">A snapshot from the configured node.</p>
		<div class="preview-actions">
			<a href="/mempool">Explore mempool <span aria-hidden="true">↗</span></a>
			<button
				type="button"
				onclick={() => refresh?.()}
				disabled={observation.loading || !refresh}
				aria-label="Refresh pending preview">{observation.loading ? 'Checking…' : 'Refresh'}</button
			>
		</div>
	</div>
	<div class="preview-observation" aria-live="polite">
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
				Pending transactions can change before confirmation.
			</p>
		{/if}
	</div>
</section>

<style>
	.pending-preview {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: 28px;
		padding: 24px;
		margin: 24px 0;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface);
		min-width: 0;
		width: 100%;
		box-sizing: border-box;
	}
	.preview-intro,
	.preview-observation {
		min-width: 0;
	}
	.eyebrow {
		color: var(--accent-ink);
		text-transform: uppercase;
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.14em;
		margin: 0 0 8px;
	}
	h2 {
		margin: 0;
		font-size: 23px;
		letter-spacing: -0.035em;
	}
	.scope,
	.preview-time {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.6;
	}
	.scope {
		margin: 8px 0 18px;
	}
	.preview-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
		align-items: center;
	}
	.preview-actions a {
		font-weight: 600;
		font-size: 13px;
		color: var(--accent-ink);
	}
	.preview-actions button {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--fg-muted);
		padding: 6px 10px;
		cursor: pointer;
		font: inherit;
		font-size: 12px;
	}
	.preview-actions button:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.preview-count {
		display: flex;
		align-items: baseline;
		gap: 10px;
		margin-bottom: 8px;
	}
	.preview-count strong {
		font-size: 32px;
		line-height: 1;
		letter-spacing: -0.055em;
		font-variant-numeric: tabular-nums;
	}
	.preview-count span {
		color: var(--fg-muted);
		font-size: 12px;
	}
	.pending-sample {
		padding: 0;
		margin: 0;
		list-style: none;
	}
	.pending-sample li {
		display: flex;
		justify-content: space-between;
		gap: 8px 20px;
		flex-wrap: wrap;
		border-bottom: 1px solid var(--hairline);
		padding: 9px 0;
		font-size: 12px;
	}
	.pending-fee {
		color: var(--fg-muted);
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}
	.preview-message {
		font-size: 13px;
		line-height: 1.6;
		margin: 12px 0;
	}
	.preview-message strong,
	.preview-message span {
		display: block;
	}
	.preview-message span {
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.preview-time {
		margin: 12px 0 0;
		font-size: 11px;
	}
	:global([data-appearance='prism']) .pending-preview {
		padding: 0;
		overflow: hidden;
		background: linear-gradient(
			125deg,
			color-mix(in srgb, var(--accent) 9%, var(--surface)),
			var(--surface) 60%
		);
		box-shadow: 0 14px 32px -24px color-mix(in srgb, var(--fg) 30%, transparent);
	}
	:global([data-appearance='prism']) .preview-intro {
		padding: 28px;
		border-right: 1px solid var(--hairline);
	}
	:global([data-appearance='prism']) .preview-observation {
		padding: 24px 28px 24px 0;
	}
	:global([data-appearance='prism']) .preview-count strong {
		font-size: 44px;
		color: var(--accent-ink);
	}
	:global([data-appearance='atelier']) .pending-preview {
		border: 0;
		border-top: 2px solid var(--fg);
		border-bottom: 1px solid var(--fg);
		border-radius: 0;
		padding: 24px 0;
		background: transparent;
		grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
	}
	:global([data-appearance='atelier']) h2 {
		font-family: Georgia, serif;
		font-size: 29px;
		font-weight: 400;
	}
	:global([data-appearance='atelier']) .preview-count {
		border-bottom: 1px solid var(--hairline);
		padding-bottom: 14px;
		justify-content: space-between;
	}
	:global([data-appearance='aurora']) .pending-preview {
		grid-template-columns: 1fr;
		border-radius: 28px;
		padding: 32px;
		background: radial-gradient(
			ellipse at 50% 0,
			color-mix(in srgb, var(--accent) 11%, var(--surface)),
			var(--surface) 70%
		);
	}
	:global([data-appearance='aurora']) .preview-intro {
		text-align: center;
	}
	:global([data-appearance='aurora']) h2 {
		font-size: 30px;
	}
	:global([data-appearance='aurora']) .preview-actions,
	:global([data-appearance='aurora']) .preview-count {
		justify-content: center;
	}
	:global([data-appearance='aurora']) .pending-sample {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 12px;
	}
	:global([data-appearance='aurora']) .pending-sample li {
		border: 1px solid var(--hairline);
		border-radius: 14px;
		padding: 16px;
		flex-direction: column;
		background: var(--surface);
	}
	:global([data-appearance='aurora']) .preview-time {
		text-align: center;
	}
	@media (max-width: 720px) {
		.pending-preview,
		:global([data-appearance='atelier']) .pending-preview {
			grid-template-columns: 1fr;
			gap: 20px;
			padding: 20px;
		}
		:global([data-appearance='atelier']) .pending-preview {
			padding: 20px 0;
		}
		:global([data-appearance='prism']) .pending-preview {
			padding: 0;
			gap: 0;
		}
		:global([data-appearance='prism']) .preview-intro {
			padding: 20px;
			border-right: 0;
			border-bottom: 1px solid var(--hairline);
		}
		:global([data-appearance='prism']) .preview-observation {
			padding: 20px;
		}
		:global([data-appearance='aurora']) .pending-preview {
			padding: 22px 18px;
		}
		:global([data-appearance='aurora']) .pending-sample {
			grid-template-columns: 1fr;
		}
	}
</style>
