<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { SvelteURL } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import { navigating, page } from '$app/state';
	import Hash from '$lib/components/Hash.svelte';
	import { truncateMiddle } from '$lib/format/hash';
	import { formatErg, formatNano, formatTokenAmount } from '$lib/format/amount';
	import type { RentSchedulePageDto, RentScheduleBatchDto } from '$lib/api/types';
	import {
		SCHEDULE_WINDOWS,
		SCHEDULE_ROW_LIMIT,
		createScheduleLoader,
		emptySchedule,
		scheduleBatches,
		scheduleTotals,
		scheduleQuery,
		type ScheduleQuery,
		type ScheduleRanking
	} from '$lib/rent/schedule';
	let {
		query,
		initial,
		error
	}: { query: ScheduleQuery; initial: RentSchedulePageDto | null; error: string | null } = $props();
	const uid = $props.id();
	let schedule = $state(emptySchedule());
	let tokenDraft = $state('');
	let modeDraft = $state('all');
	let filterError = $state<string | null>(null);
	let filtersOpen = $state(false);
	let showAllBatches = $state(false);
	let ranking = $state<ScheduleRanking>('rent');
	let now = $state(Date.now());
	const loader = createScheduleLoader((next) => (schedule = next));
	$effect(() => {
		const seed = { query, initial, error };
		untrack(() => {
			loader.seed(seed.query, seed.initial, seed.error);
			tokenDraft = seed.query.token_id ?? '';
			modeDraft = seed.query.mode;
			filterError = null;
			filtersOpen = seed.query.mode !== 'all' || !!seed.query.token_id;
			showAllBatches = false;
			ranking = 'rent';
			now = Date.now();
		});
	});
	$effect(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 60_000);
		return () => clearInterval(timer);
	});
	onDestroy(loader.stop);
	const first = $derived(schedule.first);
	const context = $derived(first?.schedule_context);
	const overview = $derived(first?.batches ?? []);
	const totals = $derived(scheduleTotals(overview));
	const batches = $derived(scheduleBatches(overview, query.window, ranking));
	const ranked = $derived(showAllBatches ? batches : batches.slice(0, 3));
	const fullOverview = $derived(context?.batch_complete === true && context.full_history);
	const oldAnchor = $derived(context ? now - context.anchor_timestamp_ms > 3_600_000 : false);
	const changing = $derived(navigating.to?.url.pathname === '/rent');
	const busy = $derived(schedule.busy || changing);
	const erg = (value: string | bigint) => formatErg(value, { maxFrac: 9 });
	const utc = (value: number) => new Date(value).toISOString().slice(0, 16).replace('T', ' ');
	const day = (value: number) => new Date(value).toISOString().slice(0, 10);
	const hour = (value: number) => new Date(value).toISOString().slice(11, 16);
	const rankAmount = (batch: RentScheduleBatchDto) =>
		ranking === 'boxes'
			? BigInt(batch.box_count)
			: ranking === 'full_claims'
				? BigInt(batch.full_claim_count)
				: ranking === 'token'
					? BigInt(batch.selected_token_full_claim_amount ?? '0')
					: BigInt(batch.collectible_due_nano);
	const peakWidth = (batch: RentScheduleBatchDto) => {
		const maximum = batches[0] ? rankAmount(batches[0]) : 0n;
		return maximum === 0n ? 0 : Number((rankAmount(batch) * 1000n) / maximum) / 10;
	};
	async function change(window = query.window) {
		const url = new SvelteURL(page.url);
		url.searchParams.set('window', window);
		url.searchParams.set('mode', modeDraft);
		const token = tokenDraft.trim().toLowerCase();
		if (token) url.searchParams.set('token_id', token);
		else url.searchParams.delete('token_id');
		const parsed = scheduleQuery(url.searchParams);
		filterError = parsed.error;
		if (filterError) return;
		url.hash = 'upcoming';
		if (
			parsed.query.window === query.window &&
			parsed.query.mode === query.mode &&
			parsed.query.token_id === query.token_id
		) {
			await loader.refresh();
		} else {
			await goto(url.pathname + url.search + url.hash, { noScroll: true, keepFocus: true });
		}
	}
</script>

<section class="rent-calendar" aria-label="Upcoming rent calendar" aria-busy={busy}>
	<div class="schedule-toolbar">
		<div class="windows" role="group" aria-label="Calendar horizon">
			{#each SCHEDULE_WINDOWS as window (window)}
				<button
					type="button"
					aria-pressed={query.window === window}
					disabled={busy}
					onclick={() => change(window)}>{window}</button
				>
			{/each}
		</div>
		<button
			class="filter-toggle"
			type="button"
			aria-label="Filter boxes"
			aria-expanded={filtersOpen}
			aria-controls={`${uid}-filters`}
			onclick={() => (filtersOpen = !filtersOpen)}
			>Filters{query.mode !== 'all' || query.token_id ? ' · active' : ''}</button
		>
		<form
			class="schedule-filter"
			class:filters-open={filtersOpen}
			id={`${uid}-filters`}
			onsubmit={(event) => {
				event.preventDefault();
				void change();
			}}
		>
			<label class="mode-field" for={`${uid}-mode`}
				><span>Opportunity</span><select id={`${uid}-mode`} bind:value={modeDraft} disabled={busy}>
					<option value="all">All maturing boxes</option><option value="collectible"
						>Collectible charge</option
					><option value="full_claim">Full-claim candidates</option>
				</select></label
			>
			<label class="token-field" for={`${uid}-token`}
				><span>Token ID <small>optional</small></span><input
					id={`${uid}-token`}
					bind:value={tokenDraft}
					placeholder="64-character token ID"
					maxlength="64"
					spellcheck="false"
					autocomplete="off"
					disabled={busy}
					aria-invalid={filterError ? 'true' : undefined}
				/></label
			>
			<button class="apply" type="submit" disabled={busy}>{busy ? 'Loading…' : 'Apply'}</button>
		</form>
	</div>
	{#if filterError}<p class="schedule-warning" role="alert">{filterError}</p>{/if}
	{#if schedule.error}<div class="schedule-warning" role="alert">
			<span
				>{schedule.error}{#if schedule.stale}
					Previous snapshot retained; these records and totals are stale.{/if}</span
			><button type="button" disabled={busy} onclick={() => loader.refresh()}>Retry schedule</button
			>
		</div>{/if}
	{#if first && context}
		<div class="schedule-context">
			<span
				>{schedule.stale ? 'Stale snapshot' : 'Snapshot'}
				<a href={`/blocks/${first.anchor!.block_id}`}
					>{first.anchor!.height.toLocaleString('en-US')}</a
				>
				·
				<time datetime={new Date(context.anchor_timestamp_ms).toISOString()}
					>{utc(context.anchor_timestamp_ms)} UTC</time
				></span
			>
			<button
				type="button"
				disabled={busy}
				onclick={() => loader.refresh()}
				aria-label="Refresh rent schedule">Refresh</button
			>
		</div>
		<p class="estimate-note">
			UTC estimates · {context.target_block_seconds}s/block · claims not automatic.
		</p>
		{#if oldAnchor}<p class="schedule-warning">
				The indexed block is more than an hour old. Dates project forward from that block, not from
				now; some estimates may already be in the past.
			</p>{/if}
		{#if !context.full_history}<p class="schedule-warning">
				Partial index · observed boxes only.
			</p>{/if}
		{#if context.batch_complete !== true}<p class="schedule-warning">
				Batch scan incomplete ({context.batch_scanned?.toLocaleString('en-US') ?? 'unknown'} candidates
				scanned{#if context.batch_stop_reason}, {context.batch_stop_reason.replaceAll(
						'_',
						' '
					)}{/if}). Amounts are observed lower bounds; larger batches may be missing.
			</p>{/if}
		<div class="schedule-metrics" aria-label="Window overview">
			<div>
				<span>{fullOverview ? 'Window collectible rent' : 'Observed rent'}</span><strong
					title={`${totals.due} nanoERG`}>{erg(totals.due)} <small>ERG</small></strong
				>
			</div>
			<div>
				<span>{fullOverview ? 'Matching boxes' : 'Observed boxes'}</span><strong
					>{totals.boxes.toLocaleString('en-US')}</strong
				>
			</div>
			<div>
				<span>Full-claim candidates</span><strong
					>{totals.fullClaims.toLocaleString('en-US')}
					<small>· {erg(totals.fullValue)} ERG</small></strong
				>
			</div>
			{#if query.token_id}<div class="selected-token-total">
					<span>Selected token in full claims</span><strong title="Exact raw token units"
						>{formatNano(totals.selectedToken)} <small>raw units</small></strong
					>
				</div>{/if}
		</div>
		<div class="schedule-workspace">
			<section class="schedule-records" aria-labelledby={`${uid}-records`}>
				<header class="section-head">
					<h2 id={`${uid}-records`}>Soonest first</h2>
					<a class="batch-jump" href="#rent-calendar-peaks"
						>Peak {query.window === '24h' ? 'hours' : 'days'}</a
					>
					<span
						>{schedule.items.length} loaded{schedule.cursor
							? ' · more available'
							: ' · scan exhausted'}</span
					>
				</header>
				{#if schedule.items.length === 0}<p class="empty">
						{schedule.cursor
							? 'No matches in this scanned segment. Continue scanning for later candidates.'
							: context.full_history
								? 'No indexed boxes match this window and filter.'
								: 'No matching boxes were observed in this partial index.'}
					</p>{/if}
				<ol class="rent-records">
					{#each schedule.items as item (item.box_id)}
						<li class="rent-record" data-box-id={item.box_id}>
							<div class="record-time">
								<time datetime={new Date(item.estimated_maturity_ms).toISOString()}
									>{day(item.estimated_maturity_ms)}
									<strong>{hour(item.estimated_maturity_ms)} <small>UTC est.</small></strong></time
								><span
									>Block {item.maturity_height.toLocaleString('en-US')} · +{(
										item.maturity_height - first.anchor!.height
									).toLocaleString('en-US')}</span
								>
							</div>
							<div class="record-identity">
								<Hash value={item.box_id} href={`/box/${item.box_id}`} copy={false} /><span
									class:full-claim={item.full_claim}
									>{item.protocol_constrained
										? 'Protocol-constrained'
										: item.full_claim
											? 'Full-claim candidate'
											: item.collectible
												? 'Partial charge candidate'
												: 'Non-positive consensus fee'}</span
								>
							</div>
							<div class="record-amount">
								<span>Collectible charge</span><strong
									title={`${item.collectible_due_nano} nanoERG`}
									>{erg(item.collectible_due_nano)} <small>ERG</small></strong
								><span>Box value {erg(item.value)} ERG</span>
							</div>
							{#if item.tokens.length > 0}<div class="record-tokens">
									<span
										>{item.full_claim
											? 'Tokens in full-claim candidate'
											: 'Tokens held · not a token claim'}</span
									>
									{#each item.tokens.slice(0, 2) as token (token.id)}<div class="token-line">
											<strong title={`${token.amount} raw units`}
												>{formatTokenAmount(
													token.amount,
													token.decimals
												)}{#if token.decimals === null}&nbsp;<small>raw units</small>{/if}</strong
											><a class="token-reference" href={`/token/${token.id}`} title={token.id}
												><span>{token.name || 'Unnamed token'}</span><span class="token-id"
													>{truncateMiddle(token.id, 5, 4)}</span
												></a
											>
										</div>{/each}
									{#if item.tokens.length > 2}<details>
											<summary>{item.tokens.length - 2} more token IDs</summary
											>{#each item.tokens.slice(2) as token (token.id)}<div class="token-line">
													<strong title={`${token.amount} raw units`}
														>{formatTokenAmount(
															token.amount,
															token.decimals
														)}{#if token.decimals === null}&nbsp;<small>raw units</small
															>{/if}</strong
													><a class="token-reference" href={`/token/${token.id}`} title={token.id}
														><span>{token.name || 'Unnamed token'}</span><span class="token-id"
															>{truncateMiddle(token.id, 5, 4)}</span
														></a
													>
												</div>{/each}
										</details>{/if}
								</div>{/if}
						</li>
					{/each}
				</ol>
				<div class="schedule-pagination">
					{#if schedule.cursor && schedule.items.length < SCHEDULE_ROW_LIMIT}<button
							type="button"
							disabled={busy}
							onclick={() => loader.more()}
							>{busy
								? 'Loading…'
								: schedule.scanLimited || !schedule.items.length
									? 'Continue scanning'
									: 'Load next boxes'}</button
						>{:else if schedule.cursor}<p>
							Display limit: {SCHEDULE_ROW_LIMIT} boxes. Narrow the window or token filter; the independent
							batch overview retains its stated scope.
						</p>{/if}
					<span
						>{schedule.scanned.toLocaleString('en-US')} candidates scanned for this loaded list.{#if schedule.scanLimited}
							This page reached its scan limit.{/if}</span
					>
				</div>
			</section>
			<aside id="rent-calendar-peaks" class="schedule-batches" aria-labelledby={`${uid}-batches`}>
				<header class="section-head">
					<h2 id={`${uid}-batches`}>
						{fullOverview ? 'Largest' : 'Largest observed'}
						{query.window === '24h' ? 'hours' : 'days'}
					</h2>
				</header>
				<label class="rank-control" for={`${uid}-ranking`}
					><span>Rank batches by</span><select id={`${uid}-ranking`} bind:value={ranking}
						><option value="rent">Rent amount</option><option value="boxes">Box count</option
						><option value="full_claims">Full-claim boxes</option>{#if query.token_id}<option
								value="token">Selected token quantity</option
							>{/if}</select
					></label
				>
				<p class="batch-note">
					Independent of the {schedule.items.length} loaded records. Full-claim value is included in collectible
					ERG.
				</p>
				<ol class="batch-list">
					{#each ranked as batch, index (batch.estimated_hour_start_ms)}
						<li class="batch">
							<div class="batch-top">
								<span class="batch-rank">{String(index + 1).padStart(2, '0')}</span>
								<div>
									<time datetime={new Date(batch.estimated_hour_start_ms).toISOString()}
										>{query.window === '24h'
											? utc(batch.estimated_hour_start_ms)
											: day(batch.estimated_hour_start_ms)} UTC</time
									><strong>{erg(batch.collectible_due_nano)} <small>ERG</small></strong>
								</div>
							</div>
							<div class="batch-bar" aria-hidden="true">
								<span style:width={`${peakWidth(batch)}%`}></span>
							</div>
							<p>
								{batch.box_count.toLocaleString('en-US')} boxes · {batch.full_claim_count.toLocaleString(
									'en-US'
								)} full candidates
							</p>
							<p>Full-claim value: {erg(batch.full_claim_value_nano)} ERG</p>
							{#if batch.selected_token_full_claim_amount !== null}<p>
									Selected token: <strong
										>{formatNano(batch.selected_token_full_claim_amount)}</strong
									> raw units in full claims
								</p>{/if}
							{#if query.window !== '24h'}<details class="hour-breakdown">
									<summary>Hourly breakdown ({batch.hours.length})</summary>
									<ul>
										{#each batch.hours as part (part.estimated_hour_start_ms)}<li>
												<time datetime={new Date(part.estimated_hour_start_ms).toISOString()}
													>{hour(part.estimated_hour_start_ms)} UTC</time
												><strong>{erg(part.collectible_due_nano)} ERG</strong><span
													>{part.box_count} boxes · {part.full_claim_count} full candidates · {erg(
														part.full_claim_value_nano
													)} ERG full-claim value</span
												>{#if part.selected_token_full_claim_amount !== null}<span
														>{formatNano(part.selected_token_full_claim_amount)} selected-token raw units
														in full claims</span
													>{/if}
											</li>{/each}
									</ul>
								</details>{/if}
						</li>
					{/each}
				</ol>
				{#if batches.length > 3}<button
						class="all-batches"
						type="button"
						onclick={() => (showAllBatches = !showAllBatches)}
						>{showAllBatches
							? 'Show top 3'
							: `Show all ${batches.length} ${query.window === '24h' ? 'hours' : 'days'}`}</button
					>{/if}
				{#if batches.length === 0}<p class="empty">No matching batch was observed.</p>{/if}
			</aside>
		</div>
		<details class="schedule-method">
			<summary>How the calendar and claim candidates work</summary>
			<p>
				Dates use the indexed block timestamp plus {context.target_block_seconds} seconds per block, not
				a guaranteed time. Current unspent boxes may move before maturity. A positive consensus fee is
				capped by the box value; non-positive fees and known protocol-constrained re-emission boxes are
				excluded. A full-claim candidate has a fee covering its complete ERG value and may release its
				tokens, but this is not proof a claim transaction will execute. Token names are metadata, not
				verified identity; token IDs and raw units are authoritative.
			</p>
			<p>
				Fee projection assumes the current default rent factor of {context.rent_factor.toLocaleString(
					'en-US'
				)} nanoERG per serialized byte, using signed 32-bit wrapping arithmetic ({context.fee_arithmetic}).
				It is not a forecast of future protocol parameters.
			</p>
			<p>
				{#if !context.full_history && context.partial_from !== null}Indexed history starts at block {context.partial_from.toLocaleString(
						'en-US'
					)}.
				{/if}
				A partial index may omit earlier outputs and pre-index spends. The batch overview scans independently
				of the paged record list. Its coverage warnings apply to totals and rankings. Refresh may change
				the canonical anchor, membership and estimates.
			</p>
		</details>
	{:else if busy}<p class="empty" role="status">Loading the anchored rent window…</p>{/if}
</section>

<style>
	.rent-calendar {
		width: 100%;
		min-width: 0;
		grid-column: 1 / -1;
		padding: 0 12px 12px;
		box-sizing: border-box;
	}
	.schedule-toolbar {
		display: flex;
		gap: 14px;
		align-items: end;
		margin-block: 12px;
		flex-wrap: wrap;
	}
	.windows {
		display: flex;
		flex: 0 0 auto;
		gap: 3px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		padding: 3px;
	}
	button,
	select,
	input,
	summary {
		font: inherit;
		font-size: 13px;
	}
	button,
	select,
	input {
		min-height: 44px;
		color: var(--fg);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
	}
	button {
		padding: 8px 13px;
		cursor: pointer;
		font-weight: 650;
	}
	button:disabled {
		opacity: 0.6;
		cursor: wait;
	}
	button:focus-visible,
	select:focus-visible,
	input:focus-visible,
	summary:focus-visible {
		outline: 2px solid var(--accent-ink);
		outline-offset: 3px;
	}
	.windows button {
		border: 0;
		background: transparent;
		min-width: 46px;
		padding-inline: 10px;
	}
	.windows button[aria-pressed='true'] {
		background: var(--accent-ink);
		color: var(--surface-solid);
	}
	.filter-toggle {
		display: none;
	}
	.batch-jump {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-height: 44px;
		font-size: 12px;
		color: var(--accent-ink);
		white-space: nowrap;
	}
	.schedule-filter {
		flex: 1;
		display: flex;
		gap: 8px;
		align-items: end;
		min-width: 0;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 11px;
		color: var(--fg-muted);
	}
	label small {
		font-size: 11px;
	}
	.mode-field {
		flex: 0 1 200px;
		min-width: 0;
	}
	.token-field {
		flex: 1;
		min-width: 130px;
	}
	input,
	select {
		width: 100%;
		min-width: 0;
		padding-inline: 10px;
	}
	input {
		font-family: var(--font-mono);
	}
	.schedule-context {
		display: flex;
		align-items: center;
		gap: 10px;
		justify-content: space-between;
		font-size: 12px;
		color: var(--fg-muted);
	}
	.schedule-context span {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.schedule-context button {
		flex: 0 0 auto;
	}
	.estimate-note,
	.batch-note {
		margin: 3px 0 10px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--fg-muted);
	}
	.schedule-warning {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		margin: 8px 0;
		border-left: 3px solid var(--warn-ink);
		padding: 8px 12px;
		color: var(--fg);
		background: var(--surface-hover);
		font-size: 12px;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	.schedule-warning button {
		flex: 0 0 auto;
	}
	.schedule-metrics {
		display: flex;
		flex-wrap: wrap;
		border-block: 1px solid var(--hairline);
		padding-block: 10px;
		gap: 10px 24px;
	}
	.schedule-metrics > div {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.schedule-metrics span,
	.record-amount > span,
	.record-tokens > span {
		color: var(--fg-muted);
		font-size: 11px;
	}
	.schedule-metrics strong {
		font-family: var(--font-number, var(--font-mono));
		font-size: 21px;
		line-height: 1.45;
		overflow-wrap: anywhere;
	}
	small {
		font-size: 11px;
		font-weight: 500;
	}
	.schedule-workspace {
		display: grid;
		gap: 20px;
		align-items: start;
	}
	.schedule-records,
	.schedule-batches {
		min-width: 0;
	}
	.section-head {
		display: flex;
		align-items: baseline;
		gap: 8px;
		justify-content: space-between;
		margin-block: 14px 5px;
	}
	h2 {
		font-size: 17px;
		font-family: var(--font-display);
	}
	.section-head > span {
		font-size: 11px;
		color: var(--fg-muted);
		text-align: right;
	}
	.rent-records,
	.batch-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.rent-record {
		display: grid;
		grid-template-columns: minmax(120px, 0.8fr) minmax(160px, 1fr) minmax(150px, 1fr);
		gap: 8px 12px;
		padding: 12px 0;
		border-bottom: 1px solid var(--hairline);
	}
	.record-time,
	.record-identity,
	.record-amount {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.record-time time {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.record-time time strong {
		display: block;
		color: var(--fg);
		font-size: 16px;
		font-family: var(--font-number, var(--font-mono));
	}
	.record-time > span,
	.record-identity > span {
		font-size: 11px;
		line-height: 1.5;
		color: var(--fg-muted);
	}
	.record-identity {
		justify-content: center;
	}
	.record-identity :global(a) {
		min-height: 32px;
		display: inline-flex;
		align-items: center;
		font-size: 13px;
	}
	.record-identity > .full-claim {
		color: var(--ok-ink);
	}
	.record-amount {
		text-align: right;
		overflow-wrap: anywhere;
	}
	.record-amount strong {
		font-size: 17px;
		font-family: var(--font-number, var(--font-mono));
	}
	.record-tokens {
		grid-column: 2 / -1;
		min-width: 0;
	}
	.token-line {
		display: flex;
		align-items: baseline;
		gap: 5px 8px;
		flex-wrap: wrap;
		font-size: 12px;
		padding-block: 3px;
		overflow-wrap: anywhere;
	}
	.token-line strong {
		font-family: var(--font-mono);
	}
	.token-line > a {
		color: var(--accent-ink);
	}
	.token-reference {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.token-id {
		font-size: 11px;
		color: var(--fg-muted);
		font-family: var(--font-mono);
	}
	summary {
		cursor: pointer;
		min-height: 44px;
		align-content: center;
		color: var(--accent-ink);
	}
	.schedule-pagination {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-top: 12px;
		flex-wrap: wrap;
	}
	.schedule-pagination span,
	.schedule-pagination p,
	.empty {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.6;
	}
	.empty {
		padding-block: 16px;
	}
	.batch-list {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 16px;
	}
	.rank-control {
		margin-block: 8px;
		max-width: 260px;
	}
	.batch {
		min-width: 0;
		padding-block: 10px;
		border-bottom: 1px solid var(--hairline);
	}
	.batch-top {
		display: flex;
		gap: 12px;
		align-items: start;
	}
	.batch-rank {
		font-family: var(--font-display);
		color: var(--fg-muted);
		font-size: 22px;
	}
	.batch-top > div {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.batch time {
		font-size: 12px;
	}
	.batch-top strong {
		font-size: 18px;
		overflow-wrap: anywhere;
		font-family: var(--font-number, var(--font-mono));
	}
	.batch p {
		color: var(--fg-muted);
		font-size: 11px;
		line-height: 1.5;
		margin-top: 4px;
		overflow-wrap: anywhere;
	}
	.batch-bar {
		height: 3px;
		margin-block: 9px;
		background: var(--hairline);
	}
	.batch-bar span {
		height: 100%;
		display: block;
		background: var(--accent-ink);
	}
	.hour-breakdown ul {
		list-style: none;
		padding: 0;
	}
	.hour-breakdown li {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 3px 8px;
		padding-block: 8px;
		border-top: 1px solid var(--hairline);
		font-size: 11px;
	}
	.hour-breakdown li strong {
		text-align: right;
		overflow-wrap: anywhere;
		font-family: var(--font-mono);
	}
	.hour-breakdown li span {
		grid-column: 1 / -1;
		color: var(--fg-muted);
	}
	.all-batches {
		margin-top: 10px;
	}
	.schedule-method {
		margin-top: 16px;
		border-top: 1px solid var(--hairline);
	}
	.schedule-method p {
		font-size: 12px;
		line-height: 1.6;
		color: var(--fg-muted);
		margin-bottom: 10px;
		max-width: 100ch;
	}
	:global(:root[data-appearance='atelier']) .rent-record {
		padding-block: 10px;
	}
	:global(:root[data-appearance='atelier']) .section-head h2 {
		font-size: 23px;
	}
	:global(:root[data-appearance='aurora']) .rent-record {
		padding: 12px;
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: 12px;
		margin-top: 8px;
	}
	@media (min-width: 1100px) {
		:global(:root[data-appearance='original']) .rent-record,
		:global(:root[data-appearance='aurora']) .rent-record {
			grid-template-columns: 110px minmax(125px, 0.8fr) minmax(110px, 0.7fr) minmax(0, 1.2fr);
			padding-block: 8px;
		}
		:global(:root[data-appearance='original']) .record-tokens,
		:global(:root[data-appearance='aurora']) .record-tokens {
			grid-column: 4;
		}
		:global(:root[data-appearance='prism']) .rent-record {
			grid-template-columns: 110px 140px 120px minmax(0, 1fr);
			padding-block: 8px;
		}
		:global(:root[data-appearance='prism']) .record-tokens {
			grid-column: 4;
		}
		:global(:root[data-appearance='prism']) .record-time time strong {
			font-size: 14px;
		}
		:global(:root[data-appearance='prism']) .record-identity {
			justify-content: start;
		}
		:global(:root[data-appearance='atelier']) .rent-record {
			grid-template-columns: 120px minmax(160px, 0.9fr) minmax(140px, 0.8fr) minmax(0, 1.2fr);
		}
		:global(:root[data-appearance='atelier']) .record-tokens {
			grid-column: 4;
			border-left: 1px solid var(--hairline);
			padding-left: 14px;
		}
		:global(:root[data-appearance='prism']) .schedule-workspace {
			grid-template-columns: minmax(0, 1fr) 255px;
		}
		:global(:root[data-appearance='prism']) .schedule-batches {
			padding-left: 18px;
			border-left: 1px solid var(--hairline);
		}
		:global(:root[data-appearance='aurora']) .schedule-workspace {
			grid-template-columns: 230px minmax(0, 1fr);
			gap: 26px;
		}
		:global(:root[data-appearance='aurora']) .schedule-batches {
			grid-column: 1;
			grid-row: 1;
		}
		:global(:root[data-appearance='aurora']) .schedule-records {
			grid-column: 2;
		}
		:global(:root[data-appearance='prism']) .batch-list,
		:global(:root[data-appearance='aurora']) .batch-list {
			grid-template-columns: 1fr;
			gap: 0;
		}
		:global(:root[data-appearance='atelier']) .schedule-batches {
			border-top: 2px solid var(--fg);
		}
		:global(:root[data-appearance='atelier']) .schedule-records > .section-head {
			margin-top: 10px;
		}
	}
	@media (max-width: 760px) {
		.schedule-toolbar {
			gap: 8px;
			margin-block: 8px;
		}
		.windows {
			flex: 1;
		}
		.windows button {
			flex: 1;
		}
		.schedule-filter {
			display: none;
			grid-template-columns: minmax(0, 1fr) auto;
			width: 100%;
			flex-basis: 100%;
		}
		.schedule-filter.filters-open {
			display: grid;
		}
		.filter-toggle {
			display: block;
			min-width: 64px;
			padding-inline: 8px;
		}
		.batch-jump {
			display: inline-flex;
			align-items: center;
			justify-content: center;
			min-height: 44px;
			font-size: 12px;
			color: var(--accent-ink);
			white-space: nowrap;
		}
		.schedule-batches {
			scroll-margin-top: 130px;
		}
		.mode-field {
			grid-column: 1 / -1;
			flex: initial;
			flex-direction: row;
			align-items: center;
			gap: 10px;
		}
		.mode-field > span {
			min-width: 72px;
		}
		.mode-field select {
			flex: 1;
		}
		.token-field {
			min-width: 0;
		}
		.schedule-metrics {
			display: grid;
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
			gap: 4px 12px;
			padding-block: 6px;
		}
		.schedule-metrics strong {
			font-size: 18px;
		}
		.schedule-metrics > div:first-child {
			flex: 1 1 50%;
		}
		.schedule-metrics > div:nth-child(3) {
			grid-column: 1 / -1;
			flex-direction: row;
			gap: 8px;
			align-items: baseline;
			width: 100%;
		}
		.schedule-metrics > div:nth-child(3) strong {
			font-size: 14px;
		}
		.rent-record {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
			gap: 6px;
			padding-block: 8px;
		}
		.record-time {
			grid-column: 1;
			grid-row: 1;
			gap: 2px;
		}
		.record-time time,
		.record-time > span {
			line-height: 1.25;
		}
		.record-time time strong {
			font-size: 14px;
			line-height: 1.25;
		}
		.record-amount {
			grid-column: 2;
			grid-row: 1;
			justify-content: center;
		}
		.record-identity {
			grid-column: 1 / -1;
			grid-row: 2;
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
			gap: 8px;
		}
		.record-identity :global(a) {
			min-height: 44px;
		}
		.record-tokens {
			grid-column: 1 / -1;
		}
		.record-amount strong {
			font-size: 16px;
		}
		.record-time > span {
			font-size: 11px;
		}
		.token-line {
			display: grid;
			grid-template-columns: minmax(0, 0.7fr) minmax(0, 1fr);
			align-items: center;
			padding-block: 2px;
		}
		.token-line > a {
			min-height: 44px;
			justify-content: center;
		}
		.batch-list {
			grid-template-columns: 1fr;
			gap: 0;
		}
		.schedule-warning {
			flex-wrap: wrap;
			margin-block: 6px;
			padding-block: 6px;
		}
		.estimate-note {
			margin-bottom: 6px;
		}
		.schedule-context {
			align-items: center;
			font-size: 11px;
		}
		.section-head {
			margin-block: 6px 3px;
		}
	}
	@media (max-width: 360px) {
		.windows {
			gap: 2px;
			padding: 0;
		}
		.windows button {
			min-width: 44px;
			padding-inline: 8px;
		}
	}
</style>
