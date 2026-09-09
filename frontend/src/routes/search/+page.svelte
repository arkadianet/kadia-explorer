<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import Panel from '$lib/components/Panel.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SearchBox from '$lib/components/SearchBox.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import BoxCard from '$lib/components/BoxCard.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { REGISTER_KEYS } from '$lib/registers/decode';
	import { checkRegisterValue, type RegisterKey } from '$lib/registers/search';
	import type { BoxDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const PAGE_SIZE = 50;

	let { data }: { data: PageData } = $props();

	const hints = {
		empty: 'Enter a block height, an id or an address.',
		'unknown-format': 'Enter a block height, an id or an address.',
		'not-found': '64-hex ids can be block, transaction or box ids. Addresses start with 9.'
	} as const;

	const hint = $derived(hints[data.reason]);

	/** The lookup the URL asks for, or null when it carries none. */
	const query = $derived(data.registers);
	/** The value in a tab-width label: long constants are cut, with the ellipsis that says so. */
	const shortValue = $derived(
		query === null ? '' : query.value.length > 16 ? `${query.value.slice(0, 16)}…` : query.value
	);
	const tip = $derived(status.current?.indexed ?? null);

	// ------------------------------------------------------------------------------- form
	// The fields start from the URL so a reloaded result page shows the query that produced
	// it, and stay editable from there.
	// `untrack`: these are the *initial* contents of two editable fields, not a view of `data`.
	// The effect below re-syncs them when the URL's lookup changes (a back/forward), which is
	// the only time the page should overwrite what somebody typed.
	let reg = $state<RegisterKey>(untrack(() => data.registers?.reg) ?? 'R4');
	let value = $state(untrack(() => data.registers?.value) ?? '');
	let formError = $state<string | null>(null);

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const checked = checkRegisterValue(value);
		if (!checked.ok) {
			formError = checked.error;
			return;
		}
		formError = null;
		// The lookup goes into the query string rather than into local state: that is what
		// makes it survive a reload and makes the result list a link somebody can send on.
		void goto(`/search?reg=${reg}&value=${checked.hex}`, { noScroll: true, keepFocus: true });
	}

	// ---------------------------------------------------------------------------- results
	// One pager per lookup: a different register or value is a different query, so the old
	// rows and cursor are dropped rather than appended to.
	let pager = $state<Pager<BoxDto> | null>(null);
	let pagerKey: string | null = null;

	$effect(() => {
		const key = query ? `${query.reg}:${query.value}` : null;
		if (key === pagerKey) return;
		pagerKey = key;
		if (query === null) {
			pager = null;
			return;
		}
		const { reg: r, value: v } = query;
		// Keep the form showing the lookup on screen — after a back/forward, the fields would
		// otherwise still hold the previous query. On a submit these are already equal, so
		// nothing is clobbered mid-typing.
		reg = r;
		value = v;
		formError = null;
		const p = createPager<BoxDto>((c) => api.boxesByRegister(r, v, c, PAGE_SIZE));
		pager = p;
		void p.loadMore();
	});
</script>

<svelte:head>
	<title>{query ? `Boxes with ${query.reg} = ${shortValue}` : 'Search'} — Ergo Explorer</title>
</svelte:head>

<Panel title="Search">
	{#if data.matches}
		<p>This ID identifies multiple entities. Choose a result:</p>
		<ul>
			{#each data.matches as match (match.kind)}
				<li>
					<a href={match.href}
						>{match.kind === 'token'
							? 'Token'
							: match.kind === 'box'
								? 'Mint input box'
								: match.kind}</a
					>
				</li>
			{/each}
		</ul>
	{:else if data.q}
		<EmptyState message={`No match for "${data.q}". ${hint}`} />
	{:else}
		<EmptyState message={hint} />
	{/if}
	<div class="retry">
		<!-- A distinct id keeps `#global-search` unique to the header box, and the header box
		     keeps sole ownership of the "/" shortcut. -->
		<SearchBox id="search-page-search" globalShortcut={false} />
	</div>
</Panel>

<!-- The other way in: not "what is this id", but "which boxes carry this value". The global
     box cannot answer that — a register value is not a unique identifier — so it gets a form
     of its own rather than a second guess at what was pasted. -->
<Panel title="Find boxes by register value">
	<form class="reg-form" onsubmit={submit}>
		<div class="field">
			<label for="reg-select">Register</label>
			<select id="reg-select" class="control" bind:value={reg}>
				{#each REGISTER_KEYS as key (key)}
					<option value={key}>{key}</option>
				{/each}
			</select>
		</div>

		<div class="field grow">
			<label for="reg-value">Value</label>
			<input
				id="reg-value"
				class="control mono"
				type="text"
				bind:value
				placeholder="0e0a4572676f2d6578706c"
				autocomplete="off"
				spellcheck="false"
				aria-invalid={formError !== null}
				aria-describedby={formError ? 'reg-error' : 'reg-help'}
				oninput={() => (formError = null)}
			/>
		</div>

		<button type="submit" class="btn btn-fill">Find boxes</button>
	</form>

	{#if formError}
		<p class="error" id="reg-error" role="alert">{formError}</p>
	{:else}
		<p class="help" id="reg-help">
			The serialised constant, in hex, exactly as the box stores it — a register's value on a box
			page can be pasted straight in.
		</p>
	{/if}

	{#if pager === null && query}
		<!-- The URL asks for a lookup the effect has not started yet (first paint of a
		     reloaded result page): hold the space rather than leaving the panel empty. -->
		<div class="results"><Skeleton /></div>
	{:else if pager}
		<div class="results">
			<p class="found">
				Boxes whose <b>{query?.reg}</b> holds
				<span class="mono val">{query?.value}</span>
			</p>
			<InfiniteList {pager} empty="No boxes carry this value.">
				{#snippet children(box: BoxDto)}
					<BoxCard {box} {tip} role="output" />
				{/snippet}
			</InfiniteList>
		</div>
	{/if}
</Panel>

<style>
	.retry {
		margin-top: var(--space-4);
	}

	/* Label above field, fields on one line down to a phone: the register is one short
	   dropdown and the value is a long hex string, so they never want equal widths. */
	.reg-form {
		display: flex;
		align-items: flex-end;
		gap: var(--space-3);
		flex-wrap: wrap;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.field.grow {
		flex: 1;
		min-width: 220px;
	}

	label {
		font-size: var(--fs-micro);
		font-weight: 600;
		color: var(--fg-muted);
	}

	/* The value is a hex string, so the field keeps the mono face `.control`'s font shorthand
	   would otherwise reset. */
	input.control {
		font-family: var(--font-mono);
	}

	.reg-form .btn {
		height: 34px;
		padding: 0 var(--space-5);
		font-size: var(--fs-data);
	}

	.help,
	.error {
		margin-top: var(--space-3);
		font-size: var(--fs-micro);
		max-width: 72ch;
	}

	.help {
		color: var(--fg-muted);
	}

	.error {
		color: var(--danger-ink);
	}

	.results {
		margin-top: var(--space-4);
		border-top: var(--rule);
		padding-top: var(--space-4);
	}

	.found {
		font-size: var(--fs-data);
		color: var(--fg-muted);
		margin-bottom: var(--space-3);
		overflow-wrap: anywhere;
	}

	.found b {
		color: var(--fg);
		font-weight: 600;
	}

	.val {
		color: var(--fg);
	}

	.results :global(.list) {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
</style>
