<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import Table from '$lib/components/Table.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import { status } from '$lib/status/status.svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// The layout's status store polls every 5 s; prefer its live value once it has one,
	// falling back to the page-load snapshot so the table isn't empty on first paint.
	const current = $derived(status.current ?? data.status);

	// The load function returns the API's problem detail instead of throwing, so a down
	// indexer renders here rather than on the generic error page. A later successful poll
	// from the store supersedes it.
	const loadError = $derived(current ? null : (data.error ?? status.error));

	const tone = $derived(status.health.tone);
</script>

<svelte:head>
	<title>Status — Ergo Explorer</title>
</svelte:head>

<Panel title="Indexer status">
	<p title={status.health.detail}>{status.health.label}</p>
	<p>{status.health.detail}</p>
	{#if !current}
		<ErrorState error={loadError} />
	{:else}
		<div class="fields">
			<Table>
				{#snippet head()}
					<tr>
						<th>Field</th>
						<th>Value</th>
					</tr>
				{/snippet}
				<tr>
					<td>Indexed</td>
					<td class="mono">{current.indexed ?? '—'}</td>
				</tr>
				<tr>
					<td>Best (node)</td>
					<td class="mono">{current.best}</td>
				</tr>
				<tr>
					<td>Lag</td>
					<td><Badge {tone}>{current.lag_blocks} blocks</Badge></td>
				</tr>
				<tr>
					<td>Mode</td>
					<td class="mono">{current.mode}</td>
				</tr>
				<tr>
					<td>Source</td>
					<td class="mono">{current.source}</td>
				</tr>
				<tr>
					<td>Halted</td>
					<td class="mono">{current.halted ?? '—'}</td>
				</tr>
				<tr>
					<td>Reads in flight</td>
					<td class="mono">{current.inflight_reads}</td>
				</tr>
				<tr>
					<td>Rate limited</td>
					<td class="mono">{current.rate_limited_total}</td>
				</tr>
			</Table>
		</div>

		{#if current.stalled}
			<div class="stalled" role="alert">
				<p>
					Stalled at height <strong>{current.stalled.height}</strong> for
					<strong>{current.stalled.since_secs}s</strong>
				</p>
				<p class="reason">{current.stalled.reason}</p>
			</div>
		{/if}
	{/if}

	<p class="raw">
		<a href="/v1/status" target="_blank" rel="noopener noreferrer">View raw JSON</a>
	</p>
</Panel>

<style>
	/* A six-row key/value list, not a data grid: it should not stretch to the page width. */
	.fields {
		max-width: 560px;
	}
	.stalled {
		margin-top: var(--space-4);
		padding-left: var(--space-3);
		border-left: 2px solid var(--danger);
		color: var(--danger-ink);
	}
	.stalled .reason {
		color: var(--fg-muted);
		margin-top: var(--space-1);
	}
	.raw {
		margin-top: var(--space-4);
		font-size: var(--fs-data);
	}
</style>
