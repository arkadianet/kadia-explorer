<script lang="ts">
	import { status } from '$lib/status/status.svelte';

	const tone = $derived.by((): 'ok' | 'warn' | 'danger' | 'neutral' => {
		const s = status.current;
		if (!s) return 'neutral';
		if (s.halted !== null || s.stalled !== null) return 'danger';
		if (s.lag_blocks > 100) return 'danger';
		if (s.lag_blocks > 3) return 'warn';
		return 'ok';
	});

	const label = $derived(status.current ? `${status.current.indexed ?? 0}` : '…');

	const title = $derived.by((): string => {
		const s = status.current;
		if (!s) return status.error ? 'Status unavailable' : 'Loading status…';
		if (s.stalled) {
			return `Stalled at height ${s.stalled.height} for ${s.stalled.since_secs}s: ${s.stalled.reason}`;
		}
		if (s.halted) return `Halted: ${s.halted}`;
		return `Indexed ${s.indexed ?? 0} / best ${s.best} (lag ${s.lag_blocks})`;
	});
</script>

<a href="/status" class="tip {tone}" {title} aria-live="polite">
	<span class="dot" aria-hidden="true"></span>
	<span class="mono height">{label}</span>
</a>

<style>
	/* The indexed tip, always on screen: a health dot plus the height itself. It is the one
	   number that tells you whether anything else on the page is current. */
	.tip {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		padding: 0 var(--space-2);
		height: 28px;
		border: var(--rule);
		border-radius: var(--radius);
		color: var(--fg-muted);
	}

	.tip:hover,
	.tip:focus-visible {
		color: var(--fg);
		background: var(--bg-hover);
	}

	.height {
		color: var(--fg);
	}

	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--fg-muted);
		flex-shrink: 0;
	}

	.ok .dot {
		background: var(--ok);
	}
	.warn .dot {
		background: var(--warn);
	}
	.danger .dot {
		background: var(--danger);
	}
</style>
