<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import { status } from '$lib/status/status.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// The layout's status store polls every 5 s; prefer its live value once it has one,
	// falling back to the page-load snapshot so the table isn't empty on first paint.
	const current = $derived(status.current ?? data.status);

	const tone = $derived.by((): 'ok' | 'warn' | 'danger' | 'neutral' => {
		if (current.halted !== null || current.stalled !== null) return 'danger';
		if (current.lag_blocks > 100) return 'danger';
		if (current.lag_blocks > 3) return 'warn';
		return 'ok';
	});
</script>

<svelte:head>
	<title>Status — Ergo Explorer</title>
</svelte:head>

<Panel title="Indexer status">
	<Table>
		{#snippet head()}
			<tr>
				<th>Field</th>
				<th>Value</th>
			</tr>
		{/snippet}
		<tr>
			<td>Indexed</td>
			<td>{current.indexed ?? '—'}</td>
		</tr>
		<tr>
			<td>Best (node)</td>
			<td>{current.best}</td>
		</tr>
		<tr>
			<td>Lag</td>
			<td><Badge {tone}>{current.lag_blocks} blocks</Badge></td>
		</tr>
		<tr>
			<td>Mode</td>
			<td>{current.mode}</td>
		</tr>
		<tr>
			<td>Source</td>
			<td>{current.source}</td>
		</tr>
		<tr>
			<td>Halted</td>
			<td>{current.halted ?? '—'}</td>
		</tr>
	</Table>

	{#if current.stalled}
		<div class="stalled" role="alert">
			<p>
				Stalled at height <strong>{current.stalled.height}</strong> for
				<strong>{current.stalled.since_secs}s</strong>
			</p>
			<p class="reason">{current.stalled.reason}</p>
		</div>
	{/if}

	<p class="raw">
		<a href="/v1/status" target="_blank" rel="noopener noreferrer">View raw JSON</a>
	</p>
</Panel>

<style>
	.stalled {
		margin: var(--space-3) var(--space-4) 0;
		padding: var(--space-3);
		border: 1px solid var(--danger);
		border-radius: var(--radius);
	}
	.stalled .reason {
		color: var(--fg-muted);
		margin-top: var(--space-1);
	}
	.raw {
		padding: var(--space-3) var(--space-4) var(--space-4);
	}
</style>
