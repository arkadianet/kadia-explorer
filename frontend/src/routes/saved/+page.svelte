<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import {
		savedAddresses,
		MAX_SAVED_ADDRESSES,
		MAX_BACKUP_BYTES,
		type SavedImportPreview,
		type ImportPolicy
	} from '$lib/addresses/saved.svelte';
	import SaveAddress from '$lib/components/SaveAddress.svelte';
	let announcement = $state('');
	let filter = $state('');
	let groupFilter = $state('all');
	const groups = $derived(
		savedAddresses.items
			.map((item) => item.group ?? '')
			.filter((name, index, all) => name && all.indexOf(name) === index)
			.sort((a, b) => a.localeCompare(b))
	);
	$effect(() => {
		if (groupFilter.startsWith('name:') && !groups.includes(groupFilter.slice(5)))
			groupFilter = 'all';
	});
	let filterInput: HTMLInputElement;
	const query = $derived(filter.trim().toLowerCase());
	const visibleItems = $derived(
		savedAddresses.items.filter(
			(item) =>
				(groupFilter === 'all' ||
					(groupFilter === 'ungrouped' ? !item.group : item.group === groupFilter.slice(5))) &&
				(item.label.toLowerCase().includes(query) ||
					item.address.toLowerCase().includes(query) ||
					(item.group ?? '').toLowerCase().includes(query))
		)
	);
	let heading: HTMLHeadingElement;
	function clearFilter() {
		filter = '';
		groupFilter = 'all';
		filterInput.focus();
	}
	let review = $state<SavedImportPreview | null>(null);
	let policy = $state<ImportPolicy>('keep-existing');
	let backupRaw = $state<string | null>(null);
	let fileError = $state('');
	let reading = $state(false);
	let fileInput: HTMLInputElement;
	let previewHeading = $state<HTMLHeadingElement>();
	let fileGeneration = 0;
	onDestroy(() => {
		fileGeneration++;
	});
	async function showPreview(raw: string) {
		policy = 'keep-existing';
		review = savedAddresses.previewImport(raw);
		if (review) {
			await tick();
			previewHeading?.focus();
		}
	}
	async function chooseBackup(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		input.value = '';
		const request = ++fileGeneration;
		review = null;
		backupRaw = null;
		fileError = '';
		reading = true;
		try {
			if (file.size > MAX_BACKUP_BYTES) throw new Error('Backup exceeds the 512,000-byte limit.');
			const raw = await file.text();
			if (request !== fileGeneration) return;
			backupRaw = raw;
			await showPreview(raw);
		} catch (error) {
			if (request === fileGeneration)
				fileError = error instanceof Error ? error.message : 'The backup file could not be read.';
		} finally {
			if (request === fileGeneration) reading = false;
		}
	}
	function cancelPreview() {
		fileGeneration++;
		review = null;
		backupRaw = null;
		reading = false;
		fileError = '';
		fileInput?.focus();
	}
	async function mergeBackup() {
		if (!review) return;
		if (savedAddresses.mergeImport(review, policy)) {
			announcement = `Backup merged in this browser. ${review.added} addresses added; ${review.conflicts} conflicting entries ${policy === 'keep-existing' ? 'kept unchanged' : 'updated from the backup'}.`;
			review = null;
			backupRaw = null;
			await tick();
			fileInput?.focus();
		}
	}
	function exportBackup() {
		const raw = savedAddresses.exportBackup();
		if (raw === null) return;
		const url = URL.createObjectURL(new Blob([raw], { type: 'application/json;charset=utf-8' }));
		const link = document.createElement('a');
		link.href = url;
		link.download = `kadia-saved-addresses-${new Date().toISOString().slice(0, 10)}.json`;
		link.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
		announcement = `Exported ${savedAddresses.items.length} saved addresses with their local labels and groups.`;
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
			{savedAddresses.items.length} / {MAX_SAVED_ADDRESSES} saved{#if query || groupFilter !== 'all'}
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
				placeholder="Local label, address or group"
				autocomplete="off"
				spellcheck="false"
				aria-describedby="saved-filter-help"
			/>
			<p id="saved-filter-help">
				Search only the labels, addresses and groups saved in this browser.
			</p>
			<label for="saved-group-filter">Filter by group</label>
			<select id="saved-group-filter" class="control" bind:value={groupFilter}>
				<option value="all">All groups ({savedAddresses.items.length})</option>
				<option value="ungrouped"
					>Ungrouped ({savedAddresses.items.filter((item) => !item.group).length})</option
				>
				{#each groups as group (group)}<option value={`name:${group}`}
						>{group} ({savedAddresses.items.filter((item) => item.group === group).length})</option
					>{/each}
			</select>
		</div>
		<details class="backup-tools">
			<summary>Backup and restore</summary>
			<p>
				Download a JSON copy of all saved addresses, labels and groups. The file is plain text and
				is not encrypted; keep it private. Importing stays in this browser and makes no chain
				requests.
			</p>
			<button
				type="button"
				class="backup-button"
				onclick={exportBackup}
				disabled={!savedAddresses.ready || !savedAddresses.readable}>Export JSON backup</button
			>
			<label for="saved-backup-file">Choose JSON backup</label>
			<input
				id="saved-backup-file"
				type="file"
				accept="application/json,.json"
				bind:this={fileInput}
				onchange={(event) => void chooseBackup(event)}
				disabled={!savedAddresses.ready}
				aria-describedby="backup-limits"
			/>
			<p id="backup-limits">
				Up to 512,000 bytes and 100 addresses. Review before merging; this never replaces the whole
				list. Imported addresses are checked for format only, not chain validity or ownership.
			</p>
			{#if reading}<p role="status">Reading the backup locally…</p>{/if}
			{#if fileError}<p class="notice" role="alert">{fileError}</p>{/if}
			{#if review}
				<section class="import-preview" aria-labelledby="backup-preview-title">
					<h2 id="backup-preview-title" tabindex="-1" bind:this={previewHeading}>Review backup</h2>
					<p>
						{review.added} new · {review.conflicts} conflicting · {review.unchanged} unchanged. Nothing
						has been saved yet.
					</p>
					{#if review.conflicts}<fieldset>
							<legend>Existing address conflicts</legend>
							<label
								><input
									type="radio"
									name="backup-policy"
									value="keep-existing"
									bind:group={policy}
								/>Keep existing labels and groups</label
							>
							<label
								><input
									type="radio"
									name="backup-policy"
									value="use-imported"
									bind:group={policy}
								/>Use backup labels and groups, including blank values</label
							>
						</fieldset>{/if}
					<!-- svelte-ignore a11y_no_noninteractive_tabindex (This bounded scroll area must be keyboard reachable to review every entry.) -->
					<div class="preview-rows" tabindex="0" role="region" aria-label="Backup entries">
						{#each review.rows as row (row.incoming.address)}<article>
								<strong
									>{row.action === 'add'
										? 'Add address'
										: row.action === 'unchanged'
											? 'Leave unchanged'
											: policy === 'keep-existing'
												? 'Keep existing'
												: 'Update from backup'}</strong
								>
								<code>{row.incoming.address}</code>
								{#if row.existing}<p>
										Existing: <bdi>{row.existing.label || 'No label'}</bdi> ·
										<bdi>{row.existing.group || 'Ungrouped'}</bdi>
									</p>{/if}
								<p>
									Backup: <bdi>{row.incoming.label || 'No label'}</bdi> ·
									<bdi>{row.incoming.group || 'Ungrouped'}</bdi>
								</p>
							</article>{/each}
					</div>
					<div class="backup-actions">
						<button
							type="button"
							class="backup-button"
							onclick={() => void mergeBackup()}
							disabled={!savedAddresses.readable}>Merge backup</button
						><button type="button" class="backup-button" onclick={cancelPreview}
							>Cancel preview</button
						><button
							type="button"
							class="backup-button"
							onclick={() => backupRaw && void showPreview(backupRaw)}>Preview again</button
						>
					</div>
				</section>
			{/if}
		</details>
	</header>
	<div class="saved-list">
		{#if savedAddresses.error}<p class="notice" role="alert">{savedAddresses.error}</p>{/if}
		{#if !savedAddresses.ready}<p role="status">Reading this browser’s saved addresses…</p>
		{:else if !savedAddresses.readable && !savedAddresses.items.length}<div class="empty">
				<h2>Saved data could not be read</h2>
				<p>
					The existing browser data has been preserved. Restore access to it before saving or
					merging a backup.
				</p>
			</div>
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
							<p class="group-note">Group: <bdi>{item.group || 'Ungrouped'}</bdi></p>
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
	.saved-filter input,
	.saved-filter select {
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
	.group-note {
		margin-top: 8px;
		color: var(--accent-ink);
		font-size: 11px;
		overflow-wrap: anywhere;
	}
	.backup-tools {
		margin-top: 24px;
		padding-top: 18px;
		border-top: var(--rule);
		text-align: left;
		min-width: 0;
	}
	.backup-tools summary {
		cursor: pointer;
		font-size: 14px;
		font-weight: 650;
	}
	.backup-tools p {
		margin: 12px 0;
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.7;
	}
	.backup-tools > label {
		display: block;
		margin: 18px 0 8px;
		font-size: 12px;
		font-weight: 650;
	}
	.backup-tools input[type='file'] {
		display: block;
		min-width: 0;
		max-width: 100%;
		width: 100%;
		font-size: 12px;
	}
	.backup-tools .backup-button {
		display: inline-block;
		padding: 9px 12px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		text-decoration: none;
		font-size: 12px;
		max-width: 100%;
		white-space: normal;
	}
	.backup-button:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.import-preview {
		margin-top: 22px;
		padding-top: 18px;
		border-top: var(--rule);
		min-width: 0;
	}
	.import-preview fieldset {
		min-width: 0;
		border: 0;
		padding: 0;
		margin: 16px 0;
	}
	.import-preview legend {
		font-size: 12px;
		font-weight: 650;
		margin-bottom: 10px;
	}
	.import-preview label {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		font-size: 12px;
		line-height: 1.6;
		margin: 10px 0;
	}
	.import-preview input[type='radio'] {
		flex: none;
		margin-top: 4px;
	}
	.preview-rows {
		max-height: 350px;
		overflow: auto;
		min-width: 0;
		border: var(--rule);
		border-radius: var(--radius-control);
	}
	.preview-rows article {
		padding: 12px;
		border-bottom: var(--rule);
		overflow-wrap: anywhere;
	}
	.preview-rows article:last-child {
		border-bottom: 0;
	}
	.preview-rows strong {
		font-size: 12px;
	}
	.preview-rows code {
		display: block;
		margin-top: 8px;
		font-size: 10px;
		overflow-wrap: anywhere;
	}
	.backup-actions {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
		margin-top: 16px;
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
