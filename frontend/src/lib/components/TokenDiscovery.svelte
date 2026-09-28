<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/endpoints';
	import { ApiError } from '$lib/api/client';
	import type { TokenInfoDto, TokenSearchDto } from '$lib/api/types';
	import { formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Hash from './Hash.svelte';
	import Icon from './Icon.svelte';
	import TokenBadge from './TokenBadge.svelte';

	let {
		query = '',
		match = 'prefix'
	}: {
		query?: string;
		match?: 'prefix' | 'exact';
	} = $props();
	let term = $state('');
	let mode = $state<'prefix' | 'exact'>('prefix');
	let items = $state<TokenInfoDto[]>([]);
	let metadata = $state<TokenSearchDto['search'] | null>(null);
	let cursor = $state<string | null>(null);
	let snapshot: string | undefined;
	let loading = $state(false);
	let formError = $state<string | null>(null);
	let failure = $state<unknown>(null);
	let refresh = $state(0);
	let generation = 0;
	let dialog: HTMLDialogElement;
	let modalHeading: HTMLHeadingElement;
	let opener: HTMLElement | null = null;
	let inspected = $state<TokenInfoDto | null>(null);
	let compared = $state<TokenInfoDto | null>(null);
	let view = $state<'identity' | 'compare'>('identity');
	const dialogTitle = $derived(view === 'identity' ? 'Token identity' : 'Compare token identities');
	const canRestart = $derived(failure instanceof ApiError && failure.status === 409);
	const errorTitle = $derived(
		failure instanceof ApiError && failure.code === 'token_search_preparing'
			? 'Token search is preparing'
			: failure instanceof ApiError && failure.status === 404
				? 'Name search is unavailable on this server'
				: canRestart
					? 'The chain changed'
					: failure instanceof ApiError && failure.status === 400
						? 'Check the search query'
						: 'Token search is unavailable'
	);
	const errorDescription = $derived(
		failure instanceof ApiError && failure.code === 'token_search_preparing'
			? 'The explorer is building its name index. Existing token pages remain available; try this search again shortly.'
			: canRestart
				? 'Restart this search to compare tokens from the current index.'
				: failure instanceof ApiError && failure.status === 400
					? failure.detail
					: 'We could not retrieve name matches. This is not an empty result. You can still open a token using its full ID.'
	);

	async function load(key: number, q: string, kind: 'prefix' | 'exact', next?: string) {
		if (untrack(() => loading)) return;
		loading = true;
		failure = null;
		try {
			const response = await api.tokenSearch(q, kind, next, snapshot);
			if (key !== generation) return;
			items = next ? [...items, ...response.items] : response.items;
			metadata = response.search;
			cursor = response.next_cursor;
			snapshot = response.next_snapshot ?? undefined;
		} catch (error) {
			if (key !== generation) return;
			failure = error;
			if (error instanceof ApiError && error.status === 409) {
				items = [];
				cursor = null;
				metadata = null;
				dialog?.close();
			}
		} finally {
			if (key === generation) loading = false;
		}
	}
	$effect(() => {
		const q = query;
		const kind = match;
		void refresh;
		const key = ++generation;
		term = q;
		formError = null;
		mode = kind;
		items = [];
		metadata = null;
		cursor = null;
		snapshot = undefined;
		failure = null;
		loading = false;
		dialog?.close();
		if (q)
			untrack(() => {
				void load(key, q, kind);
			});
		return () => {
			generation++;
		};
	});
	function submit(event: SubmitEvent) {
		event.preventDefault();
		// Preserve non-ASCII spacing: it is part of the minted name in the search index.
		const q = term.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, '');
		if (new TextEncoder().encode(q).length > 512) {
			formError = 'Use a shorter name or prefix (up to 512 UTF-8 bytes).';
			return;
		}
		formError = null;
		void goto(q ? `/tokens?q=${encodeURIComponent(q)}&match=${mode}` : '/tokens', {
			noScroll: true
		});
	}
	async function inspect(token: TokenInfoDto, kind: 'identity' | 'compare', event: MouseEvent) {
		opener = event.currentTarget as HTMLElement;
		inspected = token;
		compared = null;
		view = kind;
		await tick();
		dialog.showModal();
		modalHeading.focus();
	}
	function closed() {
		opener?.focus();
	}
	function dialogKeys(event: KeyboardEvent) {
		if (event.key !== 'Tab') return;
		const controls = Array.from(
			dialog.querySelectorAll<HTMLElement>(
				'button:not([disabled]), a[href], select:not([disabled]), input:not([disabled]), [tabindex="0"]'
			)
		);
		const first = controls[0],
			last = controls.at(-1);
		if (
			event.shiftKey &&
			(document.activeElement === first || document.activeElement === modalHeading)
		) {
			event.preventDefault();
			last?.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first?.focus();
		}
	}
</script>

<section class="discovery" aria-label="Token discovery">
	<div class="discovery-head">
		<div>
			<p class="eyebrow"><Icon name="token" size={15} /> TOKEN DISCOVERY</p>
			<h1>Find the name.<br /><span>Know the identity.</span></h1>
		</div>
		<p class="intro">
			Names can be copied.<br />A token ID is unique.<br /><strong>Start with the evidence.</strong>
		</p>
	</div>
	<form class="token-search" onsubmit={submit}>
		<div class="query-field">
			<label for="token-name-query">Token name</label><input
				id="token-name-query"
				bind:value={term}
				placeholder="Search a token name…"
				autocomplete="off"
				spellcheck="false"
				maxlength="512"
				aria-invalid={formError !== null}
				aria-describedby={formError ? 'token-query-error' : undefined}
			/>
		</div>
		<div class="match-field">
			<label for="token-name-match">Match</label><select id="token-name-match" bind:value={mode}
				><option value="prefix">Starts with</option><option value="exact">Exact name</option
				></select
			>
		</div>
		<button type="submit" class="search-submit"
			>Find tokens <Icon name="arrow-right" size={18} /></button
		>
	</form>
	{#if formError}<p id="token-query-error" class="search-note" role="alert">{formError}</p>{/if}
	<p class="search-note">
		Searches names recorded at mint. Matching names, holder counts and age do not verify a token.
	</p>
</section>

{#if query}
	<section class="results card" aria-label="Token search results" aria-busy={loading}>
		<header class="results-heading">
			<div>
				<p class="eyebrow">{match === 'exact' ? 'EXACT NAME MATCHES' : 'NAME PREFIX MATCHES'}</p>
				<h2><bdi dir="auto">{query}</bdi></h2>
			</div>
			<p>{items.length.toLocaleString('en-US')} loaded{cursor ? ' · more available' : ''}</p>
		</header>
		{#if metadata?.coverage === 'partial'}<p class="coverage" role="status">
				Partial name-search coverage. {#if metadata.partial_from !== null}Indexed history begins at
					block {metadata.partial_from.toLocaleString('en-US')}; earlier mints may be missing.{/if}
				{#if metadata.unindexed_tokens > 0}{metadata.unindexed_tokens.toLocaleString('en-US')} token names
					could not be indexed.{/if}
			</p>{/if}
		{#if loading && !items.length}<p class="result-message" role="status">
				Searching indexed token names…
			</p>
		{:else if !failure && !items.length}<div class="result-message">
				<h3>No indexed name matches</h3>
				<p>Try a shorter prefix, check the minted name, or open the token using its full ID.</p>
			</div>{/if}
		<div class="result-list">
			{#each items as token (token.id)}
				<article class="token-result" aria-label={`Token ${token.id}`}>
					<div class="token-symbol" aria-hidden="true">
						{token.name.trim().slice(0, 1).toUpperCase() || 'Σ'}
					</div>
					<div class="token-identity">
						<div class="name-line">
							<a href={`/token/${token.id}`}
								><bdi dir="auto">{token.name || 'Unnamed token'}</bdi></a
							><TokenBadge kind={token.kind} />
						</div>
						<p class="identity-label">TOKEN ID</p>
						<Hash value={token.id} head={12} tail={8} copyLabel={`Copy token ID ${token.id}`} />
						<p class="mint">
							Minted at <a href={`/blocks/${token.mint_height}`}
								>block {token.mint_height.toLocaleString('en-US')}</a
							>
						</p>
					</div>
					<div class="holders">
						<strong>{token.holder_count.toLocaleString('en-US')}</strong><span
							>holding addresses</span
						>
					</div>
					<div class="result-actions">
						<button
							type="button"
							class="inspect"
							onclick={(event) => inspect(token, 'identity', event)}
							>Inspect identity <Icon name="arrow-right" size={14} /></button
						><button
							type="button"
							class="compare"
							onclick={(event) => inspect(token, 'compare', event)}>Compare</button
						>
					</div>
				</article>
			{/each}
		</div>
		{#if failure}<div class="result-message" role="alert">
				<h3>{errorTitle}</h3>
				<p>{errorDescription}</p>
				<button
					class="btn btn-ghost"
					type="button"
					onclick={() =>
						canRestart ? refresh++ : load(generation, query, match, cursor ?? undefined)}
					>{canRestart ? 'Restart search' : 'Retry search'}</button
				>
			</div>{/if}
		{#if cursor && !failure}<div class="result-more">
				<button
					class="btn btn-ghost"
					type="button"
					disabled={loading}
					onclick={() => load(generation, query, match, cursor!)}
					>{loading ? 'Loading…' : 'Load more tokens'}</button
				>
			</div>{/if}
	</section>
{/if}

<dialog
	bind:this={dialog}
	onclose={closed}
	onkeydown={dialogKeys}
	aria-labelledby="token-identity-heading"
>
	<header class="dialog-heading">
		<div>
			<p class="eyebrow">VERIFY THE ID, NOT THE NAME</p>
			<h2 id="token-identity-heading" tabindex="-1" bind:this={modalHeading}>{dialogTitle}</h2>
		</div>
		<button type="button" class="close-dialog" onclick={() => dialog.close()}
			>Close <span aria-hidden="true">×</span></button
		>
	</header>
	<div class="dialog-body">
		{#if inspected}
			<p class="identity-caveat">
				On-chain mint metadata. These details establish identity and provenance; they do not
				establish that the token is genuine or endorsed.
			</p>
			{#if view === 'compare'}<div class="compare-picker">
					<label for="comparison-token">Compare with another loaded result</label><select
						id="comparison-token"
						value={compared?.id ?? ''}
						onchange={(event) =>
							(compared = items.find((token) => token.id === event.currentTarget.value) ?? null)}
						><option value="">Choose a second token</option
						>{#each items.filter((token) => token.id !== inspected!.id) as token (token.id)}<option
								value={token.id}
								>{token.name || 'Unnamed token'} · {truncateMiddle(token.id, 8, 6)}</option
							>{/each}</select
					>{#if items.length < 2}<p>
							Close this dialog and search a broader prefix to load more candidates.
						</p>{/if}
				</div>{/if}
			<div class="identity-grid" class:paired={compared !== null}>
				{#each compared ? [inspected, compared] : [inspected] as token (token.id)}
					<article class="identity-card">
						<div class="identity-card-head">
							<TokenBadge kind={token.kind} />
							<h3><bdi dir="auto">{token.name || 'Unnamed token'}</bdi></h3>
							<p>MINTED NAME · NOT VERIFIED</p>
						</div>
						<dl>
							<div class="full-id">
								<dt>Token ID</dt>
								<dd>
									<Hash
										value={token.id}
										head={64}
										tail={0}
										copyLabel={`Copy token ID ${token.id}`}
									/>
								</dd>
							</div>
							<div class="mint-transaction">
								<dt>Mint transaction</dt>
								<dd>
									<Hash value={token.mint_tx} href={`/tx/${token.mint_tx}`} head={12} tail={8} />
								</dd>
							</div>
							<div>
								<dt>Minted at block</dt>
								<dd>
									<a href={`/blocks/${token.mint_height}`}
										>{token.mint_height.toLocaleString('en-US')}</a
									>
								</dd>
							</div>
							<div>
								<dt>Holding addresses</dt>
								<dd>{token.holder_count.toLocaleString('en-US')}</dd>
							</div>
							<div>
								<dt>Decimals</dt>
								<dd>{token.decimals ?? 'Not declared'}</dd>
							</div>
							<div>
								<dt>Indexed supply</dt>
								<dd>
									{formatTokenAmount(token.supply, token.decimals)}{token.decimals === null
										? ' raw units'
										: ''}
								</dd>
							</div>
						</dl>
						<a class="open-token" href={`/token/${token.id}`}
							>Open token explorer <Icon name="arrow-right" size={16} /></a
						>
					</article>
				{/each}
			</div>
		{/if}
	</div>
</dialog>

<style>
	.discovery {
		padding: 30px;
		border-radius: 20px;
		background: var(--hero-bg, #103a28);
		color: var(--hero-fg, #eff8e8);
		border-top: 4px solid var(--hero-highlight, #b7f25f);
	}
	.discovery-head {
		display: flex;
		justify-content: space-between;
		gap: 28px;
		align-items: flex-end;
	}
	.eyebrow {
		font: 10px var(--font-mono);
		letter-spacing: 0.12em;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.discovery .eyebrow {
		color: var(--hero-highlight, #c3e89a);
	}
	h1 {
		font-size: clamp(30px, 4vw, 46px);
		letter-spacing: -0.055em;
		line-height: 1.1;
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 800);
		margin-top: 14px;
	}
	h1 span {
		color: var(--hero-highlight, #caff8b);
	}
	.intro {
		font-size: 13px;
		line-height: 1.8;
		color: var(--hero-muted, #c4d8c8);
	}
	.intro strong {
		font-weight: 500;
		color: var(--hero-fg, #eff8e8);
	}
	.token-search {
		display: flex;
		gap: 0;
		padding: 8px;
		background: var(--search-surface, #f8fcf3);
		border-radius: 12px;
		color: var(--search-fg, #10291e);
		margin-top: 28px;
	}
	.query-field {
		flex: 1;
		min-width: 0;
		padding: 4px 12px;
	}
	.match-field {
		width: 146px;
		border-left: 1px solid var(--search-line, #d4e0d7);
		padding: 4px 12px;
	}
	.token-search label {
		display: block;
		font: 10px var(--font-mono);
		color: var(--search-muted, #52665a);
		text-transform: uppercase;
		margin-bottom: 4px;
	}
	.token-search input,
	.token-search select {
		width: 100%;
		font: inherit;
		background: transparent;
		border: 0;
		color: var(--search-fg, #10291e);
		min-height: 28px;
	}
	.token-search input::placeholder {
		color: var(--search-muted, #52665a);
	}
	.search-submit {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
		border: 0;
		border-radius: 8px;
		background: var(--hero-btn-fill, #b7f25f);
		color: var(--hero-btn-fg, #10291e);
		padding: 12px 20px;
		font-weight: 750;
		cursor: pointer;
	}
	.search-note {
		margin-top: 12px;
		font-size: 11px;
		color: var(--hero-muted, #c4d8c8);
	}
	.results {
		overflow: hidden;
	}
	.results-heading {
		padding: 24px;
		display: flex;
		gap: 16px;
		align-items: center;
		justify-content: space-between;
		border-bottom: var(--rule);
	}
	.results-heading h2 {
		font-size: 25px;
		font-weight: 750;
		margin-top: 6px;
		overflow-wrap: anywhere;
	}
	.results-heading p {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.token-result {
		display: grid;
		grid-template-columns: 48px minmax(0, 1fr) auto auto;
		gap: 20px;
		align-items: center;
		padding: 24px;
		border-bottom: var(--rule);
	}
	.token-result:last-child {
		border-bottom: 0;
	}
	.token-symbol {
		display: grid;
		place-items: center;
		width: 48px;
		height: 48px;
		border: var(--rule);
		background: var(--accent-wash);
		color: var(--accent-ink);
		border-radius: 14px;
		font-size: 23px;
		font-weight: 800;
	}
	.token-identity {
		min-width: 0;
	}
	.name-line {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.name-line > a {
		font-size: 20px;
		font-weight: 800;
		letter-spacing: -0.04em;
		overflow-wrap: anywhere;
		max-width: 100%;
	}
	.identity-label {
		font: 9px var(--font-mono);
		letter-spacing: 0.08em;
		color: var(--fg-muted);
		margin-top: 12px;
	}
	.token-identity :global(.hash) {
		font-size: 12px;
	}
	.mint {
		font-size: 11px;
		color: var(--fg-muted);
		margin-top: 7px;
	}
	.holders {
		text-align: right;
	}
	.holders strong {
		display: block;
		font-size: 21px;
		font-weight: 750;
	}
	.holders span {
		display: block;
		font-size: 10px;
		color: var(--fg-muted);
	}
	.result-actions {
		display: grid;
		gap: 6px;
	}
	.result-actions button {
		border: 0;
		font-size: 12px;
		font-weight: 650;
		cursor: pointer;
	}
	.inspect {
		display: flex;
		justify-content: center;
		align-items: center;
		gap: 8px;
		color: var(--btn-fill-fg);
		background: var(--btn-fill);
		padding: 11px 14px;
		border-radius: 8px;
	}
	.compare {
		padding: 8px;
		background: transparent;
		color: var(--accent-ink);
	}
	.result-message,
	.result-more {
		padding: 24px;
	}
	.result-message h3 {
		font-size: 18px;
	}
	.result-message p {
		font-size: 13px;
		color: var(--fg-muted);
		margin-top: 8px;
	}
	.result-message button {
		margin-top: 16px;
	}
	.coverage {
		padding: 12px 24px;
		background: var(--accent-wash);
		color: var(--accent-ink);
		font-size: 12px;
	}
	dialog {
		width: min(900px, calc(100vw - 32px));
		max-height: calc(100dvh - 40px);
		padding: 0;
		border: 1px solid var(--hairline);
		border-radius: 18px;
		background: var(--surface-solid);
		color: var(--fg);
		box-shadow: 0 24px 100px #0006;
	}
	dialog::backdrop {
		background: var(--modal-backdrop, #03160ecc);
		backdrop-filter: blur(5px);
	}
	.dialog-heading {
		position: sticky;
		top: 0;
		z-index: 1;
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 20px;
		padding: 22px 24px;
		border-bottom: var(--rule);
		background: var(--surface-solid);
	}
	.dialog-heading .eyebrow {
		color: var(--accent-ink);
	}
	.dialog-heading h2 {
		font-size: 25px;
		letter-spacing: -0.04em;
		font-weight: 800;
		margin-top: 8px;
	}
	.close-dialog {
		flex: none;
		display: flex;
		align-items: center;
		gap: 10px;
		border: var(--rule);
		border-radius: 8px;
		padding: 8px 12px;
		background: var(--bg);
		color: var(--fg);
		font-size: 12px;
		cursor: pointer;
	}
	.close-dialog span {
		font-size: 21px;
		line-height: 1;
	}
	.dialog-body {
		padding: 24px;
	}
	.identity-caveat {
		font-size: 13px;
		color: var(--fg-muted);
		margin-bottom: 20px;
		max-width: 75ch;
	}
	.compare-picker {
		margin-bottom: 20px;
	}
	.compare-picker label {
		display: block;
		font-size: 12px;
		margin-bottom: 8px;
	}
	.compare-picker select {
		width: 100%;
		background: var(--bg);
		color: var(--fg);
		border: var(--rule);
		font: inherit;
		padding: 12px;
		border-radius: 8px;
	}
	.compare-picker p {
		font-size: 12px;
		color: var(--fg-muted);
		margin-top: 8px;
	}
	.identity-grid {
		display: grid;
		gap: 16px;
	}
	.identity-grid.paired {
		grid-template-columns: repeat(2, minmax(0, 1fr));
	}
	.identity-card {
		border: var(--rule);
		border-radius: 12px;
		overflow: hidden;
		min-width: 0;
	}
	.identity-card-head {
		padding: 22px;
		background: var(--hero-bg, #103a28);
		color: var(--hero-fg, #eff8e8);
	}
	.identity-card h3 {
		font-size: 26px;
		font-weight: 800;
		letter-spacing: -0.04em;
		margin: 12px 0 8px;
		overflow-wrap: anywhere;
	}
	.identity-card-head p {
		font: 9px var(--font-mono);
		letter-spacing: 0.1em;
		color: var(--hero-muted, #c4d8c8);
	}
	dl {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		column-gap: 20px;
		margin: 0;
		padding: 0 20px;
	}
	dl > div {
		min-width: 0;
		padding: 12px 0;
		border-bottom: var(--rule);
	}
	.full-id,
	.mint-transaction {
		grid-column: 1 / -1;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 5px;
	}
	dd {
		margin: 0;
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.full-id :global(.hash),
	.full-id :global(.mono) {
		display: inline;
		overflow-wrap: anywhere;
	}
	.open-token {
		padding: 18px 20px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		color: var(--accent-ink);
		font-size: 13px;
		font-weight: 700;
	}
	@media (max-width: 700px) {
		.discovery {
			padding: 22px;
		}
		.intro {
			display: none;
		}
		.token-search {
			flex-wrap: wrap;
			gap: 8px;
		}
		.query-field {
			flex-basis: 100%;
		}
		.match-field {
			border: 0;
			flex: 1;
		}
		.search-submit {
			padding: 10px 14px;
			font-size: 13px;
		}
		.token-result {
			grid-template-columns: 36px minmax(0, 1fr);
			gap: 12px;
			padding: 20px;
		}
		.token-symbol {
			width: 36px;
			height: 36px;
			font-size: 18px;
			border-radius: 10px;
		}
		.holders {
			grid-column: 2;
			text-align: left;
		}
		.holders strong,
		.holders span {
			display: inline;
			margin-right: 6px;
			font-size: 12px;
		}
		.result-actions {
			grid-column: 2;
			display: flex;
		}
		.results-heading {
			padding: 20px;
		}
		.identity-grid.paired {
			grid-template-columns: minmax(0, 1fr);
		}
		.dialog-heading,
		.dialog-body {
			padding: 16px;
		}
		.dialog-heading h2 {
			font-size: 21px;
		}
		.close-dialog {
			padding: 8px;
		}
	}
	@media (max-width: 400px) {
		.match-field,
		.search-submit {
			flex-basis: 100%;
			width: 100%;
		}
		dl {
			column-gap: 12px;
			padding-inline: 14px;
		}
	}
</style>
