<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { rosenApi, rosenDeposit, type RosenDeposit } from '$lib/apps/rosen';
	import RosenWorkflow from '$lib/components/RosenWorkflow.svelte';
	let id = $state('');
	let loading = $state(false);
	let error = $state<string | null>(null);
	let deposit = $state<RosenDeposit | null>(null);
	let controller: AbortController | null = null;
	let generation = 0;
	$effect(() => {
		const query = page.url.searchParams.get('box') ?? '';
		clear();
		id = /^[a-f0-9]{64}$/i.test(query) ? query.toLowerCase() : '';
	});
	function clear() {
		generation++;
		controller?.abort();
		loading = false;
		error = null;
		deposit = null;
	}
	async function load(event: SubmitEvent) {
		event.preventDefault();
		clear();
		const requested = id.trim().toLowerCase();
		if (!/^[a-f0-9]{64}$/.test(requested)) {
			error = 'Enter a 64-character Ergo deposit box ID.';
			return;
		}
		const request = generation;
		const active = new AbortController();
		controller = active;
		const signal = active.signal;
		loading = true;
		const timer = setTimeout(() => active.abort(), 10_000);
		try {
			const box = await rosenApi.getBox(requested, signal);
			if (request !== generation) return;
			if (box.id !== requested) throw new Error('The returned box does not match this request.');
			deposit = rosenDeposit(box);
			if (!deposit)
				error =
					'This box does not match the supported native ERG Rosen deposit format. Other assets and contract versions remain unrecognized.';
		} catch (reason) {
			if (request === generation)
				error = signal.aborted
					? 'The request timed out. Retry explicitly.'
					: reason instanceof Error
						? reason.message
						: 'The deposit could not be loaded.';
		} finally {
			clearTimeout(timer);
			if (request === generation) loading = false;
		}
	}
	onDestroy(clear);
</script>

<svelte:head
	><title>Rosen bridge workflow — Kadia Explorer</title><meta
		name="description"
		content="Inspect supported Rosen native ERG deposits and indexed Ergo event evidence without inferring cross-chain completion."
	/></svelte:head
>
<div class="rosen-page">
	<a href="/applications">← Applications</a>
	<p class="eyebrow">ROSEN / ERGO-SIDE EVIDENCE</p>
	<h1>Follow a bridge deposit.</h1>
	<p>
		Inspect a supported native ERG deposit to Cardano, Ethereum or Binance. Destination-chain
		completion stays separate from the evidence recorded on Ergo.
	</p>
	<form onsubmit={load}>
		<label for="rosen-box">Deposit box ID</label>
		<div class="input-row">
			<input
				id="rosen-box"
				value={id}
				oninput={(event) => {
					clear();
					id = event.currentTarget.value;
				}}
				autocomplete="off"
				spellcheck="false"
				maxlength="64"
				placeholder="64-character Ergo box ID"
			/><button disabled={loading} type="submit"
				>{loading ? 'Loading deposit…' : 'Load deposit'}</button
			>
		</div>
	</form>
	<p class="muted">
		Loading sends only the entered box ID to Kadia. Shared links fill the form without running a
		lookup. No wallet connection or third-party requests.
	</p>
	{#if error}<p role="alert">{error}</p>{/if}
	{#if deposit}{#key deposit.box.id}<RosenWorkflow {deposit} />{/key}{/if}
</div>

<style>
	.rosen-page {
		max-width: 1100px;
		margin-inline: auto;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	a {
		color: var(--accent-ink);
		font-size: 12px;
	}
	.eyebrow {
		font: 10px var(--font-mono);
		letter-spacing: 0.12em;
		color: var(--fg-muted);
		margin-top: 32px;
	}
	h1 {
		font-size: clamp(32px, 5vw, 60px);
		line-height: 1.1;
		letter-spacing: -0.055em;
		margin-block: 16px;
	}
	p {
		max-width: 75ch;
		font-size: 13px;
		line-height: 1.8;
	}
	form {
		margin-block: 24px 10px;
	}
	label {
		display: block;
		font-size: 12px;
		margin-bottom: 8px;
	}
	.input-row {
		display: flex;
		gap: 10px;
	}
	input {
		flex: 1;
		min-width: 0;
		background: var(--surface-solid);
		color: var(--fg);
		padding: 12px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		font: 12px var(--font-mono);
	}
	button {
		padding: 12px 18px;
		color: var(--btn-fill-fg);
		background: var(--btn-fill);
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		cursor: pointer;
	}
	.muted {
		color: var(--fg-muted);
		font-size: 11px;
	}
	[role='alert'] {
		border-left: 3px solid var(--accent-ink);
		padding: 12px;
	}
	:global([data-appearance='aurora']) h1 {
		text-align: center;
	}
	:global([data-appearance='atelier']) form {
		border-block: 3px double var(--hairline);
		padding-block: 20px;
	}
	@media (max-width: 520px) {
		.input-row {
			flex-direction: column;
		}
	}
</style>
