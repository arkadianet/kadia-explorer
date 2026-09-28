<script lang="ts">
	import { tick } from 'svelte';
	import { savedAddresses, MAX_SAVED_ADDRESSES } from '$lib/addresses/saved.svelte';
	import SaveAddress from '$lib/components/SaveAddress.svelte';
	let announcement = $state('');
	let filter = $state('');
	let filterInput: HTMLInputElement;
	const query = $derived(filter.trim().toLowerCase());
	const visibleItems = $derived(
		savedAddresses.items.filter(
			(item) =>
				item.label.toLowerCase().includes(query) || item.address.toLowerCase().includes(query)
		)
	);
	let heading: HTMLHeadingElement;
	function clearFilter() {
		filter = '';
		filterInput.focus();
	}
	async function remove(address: string) {
		if (savedAddresses.remove(address)) {
			announcement = 'Address removed from this browser.';
			await tick();
			heading.focus();
		}
	}
</script>

<svelte:head><title>Saved addresses — Kadia</title></svelte:head>
<section class="saved-workspace">
	<header class="saved-intro">
		<p class="eyebrow">Your browser / your labels</p>
		<h1 bind:this={heading} tabindex="-1">Saved addresses</h1>
		<p>Return to addresses you follow. Add or edit a local label from an address page.</p>
		<p class="privacy">
			Stored only in this browser, without a wallet connection. Labels do not establish ownership or
			verification. Clearing browser data removes this list.
		</p>
		<p class="count" role="status">
			{savedAddresses.items.length} / {MAX_SAVED_ADDRESSES} saved{#if query}
				· {visibleItems.length} matching{/if}
		</p>
		<div class="saved-filter">
			<label for="saved-address-filter">Filter saved addresses</label>
			<input
				id="saved-address-filter"
				class="control"
				type="search"
				bind:this={filterInput}
				bind:value={filter}
				placeholder="Local label or address"
				autocomplete="off"
				spellcheck="false"
				aria-describedby="saved-filter-help"
			/>
			<p id="saved-filter-help">Search only the labels and addresses saved in this browser.</p>
		</div>
	</header>
	<div class="saved-list">
		{#if savedAddresses.error}<p class="notice" role="alert">{savedAddresses.error}</p>{/if}
		{#if !savedAddresses.ready}<p role="status">Reading this browser’s saved addresses…</p>
		{:else if !savedAddresses.items.length}<div class="empty">
				<h2>No saved addresses yet</h2>
				<p>
					Open a resolved address and choose Save address. Its activity is one click away from here.
				</p>
				<a href="/search">Find an address</a>
			</div>
		{:else if !visibleItems.length}<div class="empty">
				<h2>No saved addresses match</h2>
				<p>Try another local label or part of an address.</p>
				<button type="button" class="clear-filter" onclick={clearFilter}>Clear filter</button>
			</div>
		{:else}<ol>
				{#each visibleItems as item, index (item.address)}
					<li class="saved-entry">
						<span class="number" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
						<div class="entry-identity">
							<h2>
								<a href={`/address/${encodeURIComponent(item.address)}`}
									><bdi>{item.label || 'Saved address'}</bdi></a
								>
							</h2>
							<p class="address mono">{item.address}</p>
							<p class="label-note">{item.label ? 'Your local label' : 'No local label'}</p>
						</div>
						<div class="entry-actions">
							<a class="open" href={`/address/${encodeURIComponent(item.address)}#activity`}
								>Open activity ↗</a
							><SaveAddress compact address={item.address} /><button
								type="button"
								onclick={() => void remove(item.address)}
								aria-label={`Remove ${item.label || item.address}`}>Remove</button
							>
						</div>
					</li>
				{/each}
			</ol>{/if}
	</div>
</section>
<p class="announcement" role="status">{announcement}</p>

<style>
	.saved-workspace {
		display: grid;
		gap: 24px;
		min-width: 0;
	}
	.saved-intro {
		padding: 24px;
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
		min-width: 0;
	}
	.eyebrow {
		color: var(--accent-ink);
		text-transform: uppercase;
		font: 11px var(--font-mono);
		letter-spacing: 0.1em;
	}
	h1 {
		font: var(--weight-display, 750) clamp(30px, 4vw, 48px)/1.1
			var(--font-display, var(--font-sans));
		letter-spacing: -0.045em;
		margin: 12px 0 16px;
	}
	.saved-intro > p:not(.eyebrow) {
		color: var(--fg-muted);
		font-size: 13px;
		line-height: 1.7;
		margin-bottom: 16px;
	}
	.saved-intro .privacy {
		font-size: 12px;
	}
	.saved-intro .count {
		font: 12px var(--font-mono);
		color: var(--accent-ink);
	}
	.saved-list {
		min-width: 0;
	}
	.saved-filter {
		display: grid;
		gap: 8px;
		text-align: left;
	}
	.saved-filter label {
		font-size: 13px;
		font-weight: 600;
	}
	.saved-filter input {
		width: 100%;
		min-width: 0;
	}
	.saved-filter p {
		font-size: 11px;
		line-height: 1.6;
		color: var(--fg-muted);
	}
	.clear-filter {
		color: var(--accent-ink);
	}
	.saved-entry {
		display: grid;
		grid-template-columns: 26px minmax(0, 1fr);
		gap: 12px 18px;
		padding: 22px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		margin-bottom: 14px;
	}
	.number {
		font: 16px var(--font-number, var(--font-mono));
		color: var(--fg-muted);
	}
	h2 {
		font: var(--weight-display, 650) 22px var(--font-display, var(--font-sans));
		overflow-wrap: anywhere;
	}
	.address {
		overflow-wrap: anywhere;
		margin-top: 10px;
		font-size: 11px;
		color: var(--fg-muted);
	}
	.label-note {
		color: var(--fg-muted);
		font-size: 11px;
		margin-top: 8px;
	}
	.entry-actions {
		grid-column: 2;
		display: flex;
		gap: 14px;
		flex-wrap: wrap;
		align-items: center;
		font-size: 12px;
	}
	.open {
		color: var(--accent-ink);
		font-weight: 650;
	}
	button {
		border: 0;
		background: none;
		color: var(--danger-ink);
		text-decoration: underline;
		cursor: pointer;
		padding: 8px 0;
	}
	.notice {
		color: var(--danger-ink);
		padding: 18px;
		border: var(--rule);
		margin-bottom: 18px;
	}
	.empty {
		padding: 28px;
		border: var(--rule);
		background: var(--surface-solid);
		border-radius: var(--radius-card);
	}
	.empty p {
		margin: 14px 0;
		color: var(--fg-muted);
	}
	.empty a {
		color: var(--accent-ink);
		text-decoration: underline;
	}
	.announcement {
		font-size: 12px;
		color: var(--fg-muted);
	}
	:global(:root[data-appearance='prism']) .saved-workspace {
		grid-template-columns: minmax(260px, 0.8fr) minmax(0, 1.8fr);
		align-items: start;
	}
	:global(:root[data-appearance='prism']) .saved-intro {
		border-left: 4px solid var(--accent-ink);
	}
	:global(:root[data-appearance='prism']) .saved-entry {
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='atelier']) .saved-intro {
		background: none;
		border: 0;
		border-bottom: 3px double var(--fg);
		border-radius: 0;
		padding: 14px 0 24px;
	}
	:global(:root[data-appearance='atelier']) .saved-entry {
		border-radius: 0;
		border: 0;
		border-bottom: var(--rule);
		background: none;
		margin: 0;
		grid-template-columns: 44px minmax(0, 1fr) minmax(220px, 0.7fr);
	}
	:global(:root[data-appearance='atelier']) .entry-actions {
		grid-column: 3;
		align-content: start;
	}
	:global(:root[data-appearance='atelier']) h1 {
		font-size: clamp(42px, 5vw, 70px);
	}
	:global(:root[data-appearance='aurora']) .saved-intro {
		text-align: center;
		background: var(--hero-bg);
		color: var(--hero-fg);
		border-radius: 26px;
		padding: 40px;
	}
	:global(:root[data-appearance='aurora']) .saved-intro > p {
		color: var(--hero-muted);
		max-width: 70ch;
		margin-inline: auto;
	}
	:global(:root[data-appearance='aurora']) .saved-filter {
		max-width: 600px;
		margin: auto;
		text-align: left;
	}
	:global(:root[data-appearance='aurora']) .saved-filter p {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='aurora']) ol {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 22px;
	}
	:global(:root[data-appearance='aurora']) .saved-entry {
		border-radius: 24px;
		padding: 26px;
		margin: 0;
		box-shadow: var(--shadow-lift);
	}
	@media (max-width: 800px) {
		:global(:root[data-appearance='prism']) .saved-workspace,
		:global(:root[data-appearance='aurora']) ol {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .saved-entry {
			grid-template-columns: 26px minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .entry-actions {
			grid-column: 2;
		}
	}
	@media (max-width: 400px) {
		.saved-intro,
		.saved-entry,
		.empty {
			padding: 18px;
		}
		.entry-actions {
			grid-column: 1/-1;
		}
		:global(:root[data-appearance='aurora']) .saved-intro,
		:global(:root[data-appearance='aurora']) .saved-entry {
			padding: 22px;
		}
	}
</style>
