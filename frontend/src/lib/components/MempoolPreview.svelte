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
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: var(--density-gap);
		padding: var(--density-panel);
		margin: 0;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface);
		min-width: 0;
		width: 100%;
		box-sizing: border-box;
	}
	.preview-refresh {
		float: right;
		margin-left: 10px;
	}
	.preview-intro {
		display: flex;
		flex-direction: column;
		justify-content: center;
	}

	.preview-intro,
	.preview-observation {
		min-width: 0;
	}
	h2 {
		margin: 0;
		font-size: 20px;
		letter-spacing: -0.035em;
	}
	.preview-time {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.6;
	}
	.preview-actions {
		margin-top: 8px;
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		align-items: center;
	}
	.preview-actions a {
		font-weight: 600;
		font-size: 13px;
		color: var(--accent-ink);
	}
	.preview-refresh button {
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--fg-muted);
		padding: 6px 10px;
		cursor: pointer;
		font: inherit;
		font-size: 12px;
	}
	.preview-refresh button:disabled {
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
		font-size: 26px;
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
		padding: var(--density-panel);
		border-right: 1px solid var(--hairline);
	}
	:global([data-appearance='prism']) .preview-observation {
		padding: var(--density-panel) var(--density-panel) var(--density-panel) 0;
	}
	:global([data-appearance='prism']) .preview-count strong {
		font-size: 28px;
		color: var(--accent-ink);
	}
	:global([data-appearance='atelier']) .pending-preview {
		border: 0;
		border-top: 2px solid var(--fg);
		border-bottom: 1px solid var(--fg);
		border-radius: 0;
		padding: var(--density-panel) 0;
		background: transparent;
		grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
	}
	:global([data-appearance='atelier']) h2 {
		font-family: Georgia, serif;
		font-size: 24px;
		font-weight: 400;
	}
	:global([data-appearance='atelier']) .preview-count {
		border-bottom: 1px solid var(--hairline);
		padding-bottom: 8px;
		justify-content: space-between;
	}
	:global([data-appearance='aurora']) .pending-preview {
		grid-template-columns: 1fr;
		border-radius: 28px;
		padding: var(--density-panel);
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
		font-size: 24px;
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
		padding: 10px;
		flex-direction: column;
		background: var(--surface);
	}
	:global([data-appearance='aurora']) .preview-time {
		text-align: center;
	}
	@media (min-width: 721px) {
		:global([data-appearance]) .pending-preview {
			grid-template-columns: auto minmax(0, 1fr) auto auto;
			grid-template-areas:
				'title count actions refresh'
				'sample sample sample sample'
				'time time time time';
			align-items: center;
			gap: 6px var(--density-gap);
			padding: var(--density-panel);
		}
		:global([data-appearance]) .preview-intro,
		:global([data-appearance]) .preview-observation {
			display: contents;
		}
		:global([data-appearance]) h2 {
			grid-area: title;
			font-size: 20px;
			line-height: 1.2;
		}
		:global([data-appearance]) .preview-actions {
			grid-area: actions;
			margin: 0;
		}
		.preview-refresh {
			grid-area: refresh;
			float: none;
			margin: 0;
		}
		.preview-refresh button {
			min-height: var(--density-row);
		}
		:global([data-appearance]) .preview-count {
			grid-area: count;
			justify-content: start;
			gap: 6px;
			margin: 0;
			padding: 0;
			border: 0;
		}
		:global([data-appearance]) .preview-count strong {
			font-size: 22px;
		}
		:global([data-appearance]) .pending-sample {
			grid-area: sample;
			display: grid;
			grid-template-columns: repeat(3, minmax(0, 1fr));
			gap: 0;
			border-block: 1px solid var(--hairline);
		}
		:global([data-appearance]) .pending-sample li {
			flex-direction: column;
			justify-content: center;
			align-items: start;
			gap: 2px;
			padding: 6px 12px;
			border: 0;
			border-radius: 0;
			background: transparent;
			min-width: 0;
		}
		.pending-sample li + li {
			border-inline-start: 1px solid var(--hairline);
		}
		:global([data-appearance]) .pending-sample li:first-child {
			padding-inline-start: 0;
		}
		:global([data-appearance]) .preview-time {
			grid-area: time;
			margin: 0;
			text-align: left;
		}
		.preview-message {
			grid-area: sample;
			margin: 4px 0;
		}
		:global([data-appearance='atelier']) .pending-preview {
			padding-inline: 0;
		}
	}
	@media (min-width: 721px) and (max-width: 1100px) {
		:global([data-appearance]) .pending-preview {
			grid-template-columns: minmax(0, 1fr) auto auto;
			grid-template-areas:
				'title actions refresh'
				'count count count'
				'sample sample sample'
				'time time time';
		}
	}
	@media (max-width: 720px) {
		.preview-intro,
		:global([data-appearance='prism']) .preview-intro {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
			gap: 8px;
			padding-bottom: 0;
		}
		h2,
		:global([data-appearance='aurora']) h2,
		:global([data-appearance='atelier']) h2 {
			font-size: 18px;
		}
		.preview-actions {
			margin-top: 0;
		}
		.preview-actions a {
			font-size: 12px;
		}
		.preview-count {
			min-height: 44px;
			align-items: center;
			margin-bottom: 0;
		}
		.preview-count strong {
			font-size: 24px;
		}
		.pending-sample li {
			padding-block: 5px;
		}

		.pending-preview,
		:global([data-appearance='atelier']) .pending-preview {
			grid-template-columns: 1fr;
			gap: 8px;
			padding: var(--density-panel);
		}
		:global([data-appearance='atelier']) .pending-preview {
			padding: var(--density-panel) 0;
		}
		:global([data-appearance='prism']) .pending-preview {
			padding: 0;
			gap: 0;
		}
		:global([data-appearance='prism']) .preview-intro {
			padding: var(--density-panel);
			border-right: 0;
			border-bottom: 1px solid var(--hairline);
		}
		:global([data-appearance='prism']) .preview-observation {
			padding: var(--density-panel);
		}
		:global([data-appearance='aurora']) .pending-preview {
			padding: var(--density-panel);
		}
		:global([data-appearance='aurora']) .pending-sample {
			grid-template-columns: 1fr;
			gap: 0;
		}
		:global([data-appearance='aurora']) .pending-sample li {
			flex-direction: row;
			border: 0;
			border-bottom: var(--rule);
			border-radius: 0;
			padding: 7px 0;
			background: none;
		}
		.preview-actions a,
		.preview-refresh button {
			min-height: 44px;
			display: inline-flex;
			align-items: center;
		}
		.preview-time {
			margin-top: 6px;
		}
		:global([data-appearance='prism']) .preview-intro {
			border-bottom: 0;
			padding-bottom: 0;
		}
		:global([data-appearance='prism']) .preview-observation {
			padding-top: 4px;
		}
	}
</style>
