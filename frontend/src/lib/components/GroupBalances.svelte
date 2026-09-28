<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { createGroupBalances, type GroupState } from '$lib/addresses/groups';
	import { createBalanceWatch, emptyWatch, MAX_WATCH_EVENTS } from '$lib/addresses/watch';
	import type { SavedAddress } from '$lib/addresses/saved.svelte';
	import { formatErg, formatNano } from '$lib/format/amount';
	let { items, scope, available }: { items: SavedAddress[]; scope: string; available: boolean } =
		$props();
	const uid = $props.id();
	let groupState = $state<GroupState>({ busy: false, result: null, stale: false, error: null });
	const controller = createGroupBalances((next) => (groupState = next));
	let watchState = $state(emptyWatch());
	const watcher = createBalanceWatch({
		read: async () => {
			await controller.load();
			const current = controller.state;
			if (current.error || current.busy || !current.result)
				throw new Error(current.error ?? 'This selection could not be read.');
			return current.result;
		},
		onChange: (next) => (watchState = next),
		visible: () => typeof document !== 'undefined' && document.visibilityState === 'visible'
	});
	onMount(() => {
		const visibility = () => watcher.visibilityChanged();
		document.addEventListener('visibilitychange', visibility);
		return () => document.removeEventListener('visibilitychange', visibility);
	});
	const signed = (value: string, erg = false) =>
		`${BigInt(value) > 0n ? '+' : ''}${erg ? formatErg(value) : formatNano(value)}`;
	const addresses = $derived(available ? items.map((item) => item.address) : []);
	let tokenQuery = $state('');
	let tokenLimit = $state(50);
	let memberLimits = $state<Record<number, number>>({});
	$effect(() => {
		const current = addresses;
		untrack(() => {
			watcher.setSelection(current);
			controller.setAddresses(current);
		});
	});
	$effect(() => {
		void groupState.result;
		tokenQuery = '';
		tokenLimit = 50;
		memberLimits = {};
	});
	onDestroy(() => {
		watcher.destroy();
		controller.stop();
	});
	const result = $derived(
		groupState.result?.members.length === addresses.length &&
			groupState.result.members.every((member, index) => member.address === addresses[index])
			? groupState.result
			: null
	);
	const totals = $derived(result?.observed_totals);
	const tokens = $derived(
		(totals?.tokens ?? []).filter((token) => token.id.includes(tokenQuery.trim().toLowerCase()))
	);
</script>

<section class="group-dashboard" aria-labelledby={`${uid}-title`}>
	<header class="dashboard-head">
		<div>
			<p class="eyebrow">Selected scripts / one indexed snapshot</p>
			<h2 id={`${uid}-title`}>Group balances</h2>
			<p class="scope">
				<bdi>{scope}</bdi> · {items.length} saved address{items.length === 1 ? '' : 'es'}
			</p>
		</div>
		<button
			type="button"
			class="load"
			disabled={!available || !items.length || groupState.busy || watchState.active}
			onclick={() => void controller.load()}
			aria-describedby={`${uid}-privacy`}
			>{groupState.busy
				? 'Loading balances…'
				: result
					? 'Refresh group balances'
					: 'Load group balances'}</button
		>
	</header>
	<p id={`${uid}-privacy`} class="disclosure">
		Loading sends these visible addresses together to the explorer. Local labels and group names
		stay in this browser. Up to 100 addresses; this selection does not establish ownership.
	</p>
	<section class="balance-watch" aria-labelledby={`${uid}-watch-title`}>
		<div class="watch-heading">
			<div>
				<h3 id={`${uid}-watch-title`}>Watch while this page is open</h3>
				<p>
					One check per minute at most. Pauses when hidden; stops when the selected addresses change
					or you leave. No background push or saved opt-in.
				</p>
			</div>
			{#if watchState.active}<button
					type="button"
					class="watch-button"
					onclick={() => watcher.stop()}>Stop watching</button
				>
			{:else}<button
					type="button"
					class="watch-button"
					disabled={!available || !items.length || groupState.busy}
					onclick={() => watcher.start()}
					aria-describedby={`${uid}-privacy`}>Start watching group</button
				>{/if}
		</div>
		<p class="watch-status" role="status">
			{watchState.message}{#if watchState.active && watchState.nextCheck !== null}<span
					>Next check no earlier than {new Date(watchState.nextCheck).toISOString()}.</span
				>{/if}
		</p>
		{#if watchState.events.length}<details class="watch-history" open>
				<summary>Observed-change timeline ({watchState.events.length} / {MAX_WATCH_EVENTS})</summary
				>
				<p class="watch-scope">
					Changes compare complete anchored snapshots for the same scripts. A higher tip alone
					cannot rule out a reorganization between reads. These are observed balance differences,
					not transfer or ownership claims.
				</p>
				<!-- svelte-ignore a11y_no_noninteractive_tabindex (The bounded watch timeline must be keyboard scrollable.) -->
				<ol tabindex="0" aria-label="Recent group watch observations">
					{#each watchState.events as event (`${event.at}:${event.kind}`)}<li
							class:changed={event.kind === 'change'}
						>
							<div class="event-caption">
								<strong
									>{event.kind === 'change'
										? 'Observed change'
										: event.kind === 'baseline'
											? 'Baseline'
											: event.kind === 'reset'
												? 'Baseline reset'
												: event.kind === 'error'
													? 'Check failed'
													: 'Comparison unavailable'}</strong
								><time datetime={new Date(event.at).toISOString()}
									>{new Date(event.at).toISOString()}</time
								>
							</div>
							<p>{event.message}</p>
							{#if event.from || event.to}<div class="event-anchors">
									{#if event.from}<a href={`/blocks/${event.from.block_id}`}
											>From {event.from.height.toLocaleString('en-US')}</a
										>{/if}{#if event.to}<a href={`/blocks/${event.to.block_id}`}
											>To {event.to.height.toLocaleString('en-US')}</a
										>{/if}
								</div>{/if}
							{#if event.change}<p class="watch-delta">
									<strong>{signed(event.change.nano, true)} ERG</strong><span
										>{signed(event.change.nano)} nanoERG · exact observed difference</span
									>
								</p>
								<ul class="token-changes">
									{#each event.change.tokens.slice(0, 20) as token (token.id)}<li>
											<a href={`/token/${token.id}`}><code>{token.id}</code></a><strong
												>{signed(token.delta)} raw units</strong
											>
										</li>{/each}
								</ul>
								{#if event.change.tokens.length > 20}<p>
										Showing 20 of {event.change.tokens.length} changed token IDs.
									</p>{/if}{/if}
						</li>{/each}
				</ol>
			</details>{/if}
	</section>
	{#if groupState.busy}<p class="working" role="status">
			Reading one snapshot{result ? '; the previous snapshot remains below' : ''}…
		</p>{/if}
	{#if groupState.error}<p class="issue" role="alert">
			{groupState.stale ? 'Stale snapshot — refresh failed. ' : ''}{groupState.error}
		</p>{/if}
	{#if result}
		<div class="snapshot" class:stale={groupState.stale}>
			<div class="snapshot-caption">
				<strong>{groupState.stale ? 'Previous snapshot' : 'Loaded snapshot'}</strong>
				{#if result.anchor}<a href={`/blocks/${result.anchor.block_id}`}
						>Block {result.anchor.height.toLocaleString('en-US')}</a
					>
				{:else}<span
						>{result.indexed_height === null
							? 'No retained indexed tip'
							: `Indexed height ${result.indexed_height.toLocaleString('en-US')}`} · no retained block
						anchor</span
					>{/if}
				<span
					>{watchState.active
						? 'Page-open watch; checks only while visible'
						: 'No automatic refresh'}</span
				>
			</div>
			{#if !result.full_history}<p class="coverage" role="status">
					Partial index{result.partial_from === null
						? ''
						: ` from height ${result.partial_from.toLocaleString('en-US')}`}. Only indexed balances
					are observed; earlier outputs and unresolved pre-index spends can leave balances
					incomplete.
				</p>{/if}
			<div class="balance-workspace">
				<div class="balance-overview">
					<p class="eyebrow">
						{result.complete
							? 'Complete indexed balance'
							: totals
								? 'Observed balance only'
								: 'Combined balance unavailable'}
					</p>
					{#if totals}<p class="erg-total">
							<strong>{formatErg(totals.nano)}</strong> <span>ERG</span>
						</p>
						<p class="raw-total">{formatNano(totals.nano)} nanoERG · exact</p>
					{:else}<p class="unavailable">Some addresses could not be resolved.</p>
						<p class="raw-total">
							Unseen or invalid addresses are not treated as zero. Individual indexed balances
							remain available below.
						</p>{/if}
					<dl class="metrics">
						<div>
							<dt>Indexed scripts</dt>
							<dd>
								{result.resolved_script_count} <span>/ {result.requested_count} addresses</span>
							</dd>
						</div>
						<div>
							<dt>Duplicate scripts</dt>
							<dd>{result.members.filter((member) => member.status === 'duplicate').length}</dd>
						</div>
						<div>
							<dt>Token IDs</dt>
							<dd>{totals ? totals.tokens.length : 'Unavailable'}</dd>
						</div>
					</dl>
					<p class="amount-note">
						Aliases of the same script count once. Token quantities below use raw integer units,
						without assumed decimals or prices.
					</p>
				</div>
				{#if totals}<div class="token-balances">
						<h3>Combined tokens</h3>
						{#if totals.tokens.length}
							<label for={`${uid}-token-filter`}>Filter loaded token IDs</label>
							<input
								id={`${uid}-token-filter`}
								class="control"
								type="search"
								bind:value={tokenQuery}
								oninput={() => (tokenLimit = 50)}
								autocomplete="off"
								spellcheck="false"
							/>
							<ul>
								{#each tokens.slice(0, tokenLimit) as token (token.id)}<li>
										<a href={`/token/${token.id}`} class="token-id">{token.id}</a><strong
											>{formatNano(token.amount)} <span>raw units</span></strong
										>
									</li>{/each}
							</ul>
							{#if !tokens.length}<p class="empty">No loaded token IDs match.</p>{/if}
							{#if tokens.length > tokenLimit}<button
									type="button"
									class="more"
									onclick={() => (tokenLimit += 50)}
									>Show 50 more token balances ({tokens.length - tokenLimit} remaining)</button
								>{/if}
						{:else}<p class="empty">
								No token balances in this {result.complete ? 'snapshot' : 'observed scope'}.
							</p>{/if}
					</div>{/if}
			</div>
			<div class="members">
				<h3>Address breakdown <span>{result.members.length} entries</span></h3>
				<ol>
					{#each result.members as member, index (member.address)}
						<li class="member">
							<div class="identity">
								<span class="member-index">{String(index + 1).padStart(2, '0')}</span>
								<div>
									<a class="member-label" href={`/address/${encodeURIComponent(member.address)}`}
										><bdi>{items[index]?.label || 'Saved address'}</bdi></a
									>
									<p class="member-address">{member.address}</p>
								</div>
							</div>
							<div class="member-value">
								{#if member.balance}<strong>{formatErg(member.balance.nano)} ERG</strong><small
										>{formatNano(member.balance.nano)} nanoERG</small
									><span
										>{result.full_history ? 'Indexed balance' : 'Observed only'} · {member.balance
											.tokens.length} token IDs</span
									>
								{:else if member.status === 'duplicate'}<strong
										>Same script as entry {(member.duplicate_of ?? 0) + 1}</strong
									><span
										>{result.members[member.duplicate_of ?? 0]?.status === 'unseen'
											? 'Original script is unseen; balance unavailable'
											: 'Counted once in combined balances'}</span
									>
								{:else if member.status === 'unseen'}<strong>Unseen in this index</strong><span
										>Balance unavailable · not assumed zero</span
									>
								{:else}<strong>Invalid address</strong><span
										>Could not decode this saved address</span
									>{/if}
							</div>
							{#if member.balance?.tokens.length}<details class="member-tokens">
									<summary>Inspect {member.balance.tokens.length} token balances</summary>
									<ul>
										{#each member.balance.tokens.slice(0, memberLimits[index] ?? 20) as token (token.id)}<li
											>
												<a href={`/token/${token.id}`} class="token-id">{token.id}</a><strong
													>{formatNano(token.amount)} <span>raw units</span></strong
												>
											</li>{/each}
									</ul>
									{#if member.balance.tokens.length > (memberLimits[index] ?? 20)}<button
											type="button"
											class="more"
											onclick={() => (memberLimits[index] = (memberLimits[index] ?? 20) + 50)}
											>Show more tokens for entry {index + 1}</button
										>{/if}
								</details>{/if}
						</li>
					{/each}
				</ol>
			</div>
		</div>
	{:else if !groupState.busy && !groupState.error}<div class="unloaded">
			<span class="unloaded-mark" aria-hidden="true">Σ</span>
			<p>
				{items.length
					? 'Load this selection to compare member balances and see exact combined ERG and token quantities.'
					: 'Choose a group or local filter with saved addresses to load balances.'}
			</p>
		</div>{/if}
</section>

<style>
	.balance-watch {
		margin-top: 22px;
		padding: 20px 0;
		border-block: var(--rule);
		min-width: 0;
	}
	.watch-heading {
		display: flex;
		gap: 20px;
		align-items: center;
		justify-content: space-between;
	}
	.watch-heading h3 {
		margin: 0 0 8px;
	}
	.watch-heading p,
	.watch-scope {
		font-size: 11px;
		line-height: 1.75;
		color: var(--fg-muted);
		max-width: 80ch;
	}
	.watch-button {
		border: 1px solid var(--accent-ink);
		border-radius: var(--radius-control);
		color: var(--accent-ink);
		background: var(--accent-wash);
		padding: 10px 14px;
		font-size: 12px;
		flex-shrink: 0;
		max-width: 100%;
	}
	.watch-button:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.watch-status {
		margin-top: 14px;
		font: 11px/1.75 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.watch-status span {
		display: block;
		margin-top: 4px;
	}
	.watch-history {
		margin-top: 20px;
		min-width: 0;
	}
	.watch-history summary {
		cursor: pointer;
		font-size: 12px;
		font-weight: 650;
	}
	.watch-scope {
		margin: 12px 0;
	}
	.watch-history > ol {
		display: grid;
		gap: 14px;
		max-height: 460px;
		overflow: auto;
		padding: 4px;
	}
	.watch-history > ol > li {
		padding: 16px;
		border: var(--rule);
		border-radius: var(--radius-control);
		min-width: 0;
	}
	.watch-history > ol > li.changed {
		border-left: 3px solid var(--accent-ink);
	}
	.event-caption {
		display: flex;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 8px;
		font-size: 12px;
	}
	.event-caption time {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.watch-history li > p {
		margin: 10px 0 0;
		font-size: 11px;
		line-height: 1.75;
	}
	.event-anchors {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
		margin-top: 10px;
		font-size: 11px;
		color: var(--accent-ink);
	}
	.watch-history .watch-delta strong {
		font: 22px/1.5 var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
	}
	.watch-delta span {
		display: block;
		font: 11px/1.7 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.token-changes {
		margin-top: 10px;
	}
	.token-changes li {
		display: grid;
		gap: 5px;
		padding: 10px 0;
		border-top: var(--rule);
	}
	.token-changes code {
		font-size: 11px;
		overflow-wrap: anywhere;
		color: var(--accent-ink);
	}
	.token-changes strong {
		font: 12px var(--font-mono);
		overflow-wrap: anywhere;
	}
	:global(:root[data-appearance='prism']) .balance-watch {
		padding: 20px;
		background: var(--surface-hover);
		border: var(--rule);
		border-radius: 14px;
	}
	:global(:root[data-appearance='atelier']) .watch-history > ol > li {
		border: 0;
		border-bottom: var(--rule);
		border-radius: 0;
		padding-inline: 0;
	}
	:global(:root[data-appearance='aurora']) .watch-heading {
		display: block;
		text-align: center;
	}
	:global(:root[data-appearance='aurora']) .watch-heading p {
		margin-inline: auto;
	}
	:global(:root[data-appearance='aurora']) .watch-button {
		margin-top: 16px;
		border-radius: 100px;
	}
	:global(:root[data-appearance='aurora']) .watch-history > ol {
		grid-template-columns: repeat(2, minmax(0, 1fr));
	}
	@media (max-width: 650px) {
		.watch-heading {
			flex-wrap: wrap;
		}
		.watch-button {
			width: 100%;
		}
		:global(:root[data-appearance='prism']) .balance-watch {
			padding: 16px;
		}
		:global(:root[data-appearance='aurora']) .watch-history > ol {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.group-dashboard {
		min-width: 0;
		margin-bottom: 28px;
		padding: 24px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	.dashboard-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
	}
	.dashboard-head > div {
		min-width: 0;
	}
	.eyebrow {
		font: 11px var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--accent-ink);
		line-height: 1.6;
	}
	h2 {
		margin: 8px 0;
		font: var(--weight-display, 650) clamp(26px, 3vw, 40px)/1.05
			var(--font-display, var(--font-sans));
		letter-spacing: -0.035em;
	}
	.scope {
		font-size: 12px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
		line-height: 1.6;
	}
	button {
		cursor: pointer;
	}
	.load {
		flex-shrink: 0;
		border: 1px solid var(--accent-ink);
		border-radius: var(--radius-control);
		padding: 12px 16px;
		color: var(--surface-solid);
		background: var(--accent-ink);
		font: 650 12px var(--font-sans);
		max-width: 100%;
	}
	.load:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.disclosure {
		font-size: 11px;
		line-height: 1.75;
		margin: 16px 0 0;
		color: var(--fg-muted);
		max-width: 90ch;
	}
	.working,
	.coverage,
	.issue {
		margin: 18px 0 0;
		padding: 12px 14px;
		border-left: 3px solid var(--accent-ink);
		background: var(--accent-wash);
		font-size: 12px;
		line-height: 1.7;
		overflow-wrap: anywhere;
	}
	.issue {
		border-color: var(--danger-ink);
		color: var(--danger-ink);
		background: var(--surface-hover);
	}
	.coverage {
		border-color: var(--warn-ink);
		color: var(--fg);
	}
	.snapshot {
		margin-top: 24px;
		min-width: 0;
	}
	.snapshot-caption {
		display: flex;
		gap: 8px 16px;
		flex-wrap: wrap;
		font: 11px var(--font-mono);
		color: var(--fg-muted);
		line-height: 1.7;
		padding-bottom: 14px;
		border-bottom: var(--rule);
	}
	.snapshot-caption a {
		color: var(--accent-ink);
		text-decoration: underline;
		text-underline-offset: 3px;
	}
	.stale .snapshot-caption {
		color: var(--warn-ink);
	}
	.balance-workspace {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 24px;
		padding: 26px 0;
	}
	.balance-overview {
		min-width: 0;
	}
	.erg-total {
		margin: 12px 0 6px;
		overflow-wrap: anywhere;
		font-family: var(--font-number, var(--font-mono));
	}
	.erg-total strong {
		font-size: clamp(28px, 3.7vw, 48px);
		font-weight: 550;
		letter-spacing: -0.04em;
	}
	.erg-total > span {
		font-size: 14px;
		color: var(--fg-muted);
	}
	.raw-total {
		font: 11px/1.7 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.unavailable {
		margin: 14px 0 10px;
		font: 26px/1.2 var(--font-display, var(--font-sans));
	}
	.metrics {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		margin: 24px 0 16px;
		gap: 16px;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 6px;
	}
	dd {
		font: 22px var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
	}
	dd span {
		display: block;
		font-size: 11px;
		margin-top: 5px;
		color: var(--fg-muted);
	}
	.amount-note {
		font-size: 11px;
		line-height: 1.7;
		color: var(--fg-muted);
		max-width: 70ch;
	}
	h3 {
		font: 650 16px var(--font-sans);
		margin-bottom: 16px;
	}
	h3 span {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
		margin-left: 8px;
	}
	.token-balances {
		padding: 20px;
		border: var(--rule);
		border-radius: var(--radius-control);
		min-width: 0;
		background: var(--surface-hover);
	}
	.token-balances label {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 6px;
	}
	.token-balances input {
		width: 100%;
		min-width: 0;
		margin-bottom: 10px;
	}
	.token-balances li,
	.member-tokens li {
		padding: 12px 0;
		border-bottom: var(--rule);
		min-width: 0;
	}
	.token-id {
		display: block;
		overflow-wrap: anywhere;
		font: 11px/1.6 var(--font-mono);
		color: var(--accent-ink);
	}
	.token-balances li strong,
	.member-tokens li strong {
		display: block;
		margin-top: 6px;
		font: 14px/1.6 var(--font-number, var(--font-mono));
		overflow-wrap: anywhere;
	}
	li strong span {
		font: 11px var(--font-sans);
		color: var(--fg-muted);
	}
	.more {
		font-size: 12px;
		margin-top: 12px;
		padding: 8px 0;
		border: 0;
		background: transparent;
		color: var(--accent-ink);
		text-decoration: underline;
		text-align: left;
	}
	.members {
		padding-top: 20px;
		border-top: var(--rule);
	}
	.member {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(160px, 0.7fr);
		gap: 14px 24px;
		padding: 20px 0;
		border-top: var(--rule);
	}
	.identity {
		display: grid;
		grid-template-columns: 24px minmax(0, 1fr);
		gap: 12px;
	}
	.member-index {
		font: 11px var(--font-mono);
		color: var(--fg-muted);
		padding-top: 4px;
	}
	.member-label {
		font-size: 14px;
		font-weight: 650;
		overflow-wrap: anywhere;
	}
	.member-address {
		font: 11px/1.65 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
		margin-top: 7px;
	}
	.member-value {
		min-width: 0;
		display: grid;
		gap: 5px;
		align-content: start;
		overflow-wrap: anywhere;
	}
	.member-value strong {
		font: 14px/1.5 var(--font-number, var(--font-mono));
	}
	.member-value small,
	.member-value > span {
		font: 11px/1.6 var(--font-sans);
		color: var(--fg-muted);
	}
	.member-tokens {
		grid-column: 1/-1;
		min-width: 0;
		padding-left: 36px;
	}
	.member-tokens summary {
		cursor: pointer;
		color: var(--accent-ink);
		font-size: 12px;
	}
	.empty {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.unloaded {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 28px 0 8px;
		color: var(--fg-muted);
	}
	.unloaded p {
		font-size: 13px;
		line-height: 1.7;
		max-width: 55ch;
	}
	.unloaded-mark {
		color: var(--accent-ink);
		font: 52px var(--font-display, var(--font-sans));
	}
	:global(:root[data-appearance='prism']) .group-dashboard {
		padding: 26px;
		border-top: 4px solid var(--accent-ink);
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='prism']) .balance-workspace {
		grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
		align-items: start;
	}
	:global(:root[data-appearance='prism']) .token-balances {
		border-radius: 14px;
		transform: translateY(8px);
	}
	:global(:root[data-appearance='prism']) .metrics {
		grid-template-columns: minmax(0, 1fr);
		gap: 0;
	}
	:global(:root[data-appearance='prism']) .metrics > div {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 12px 0;
		border-top: var(--rule);
	}
	:global(:root[data-appearance='prism']) dd {
		font-size: 18px;
		text-align: right;
	}
	:global(:root[data-appearance='atelier']) .group-dashboard {
		padding: 28px 0;
		border: 0;
		border-top: 3px double var(--fg);
		border-bottom: 3px double var(--fg);
		background: transparent;
		border-radius: 0;
	}
	:global(:root[data-appearance='atelier']) .dashboard-head {
		align-items: baseline;
	}
	:global(:root[data-appearance='atelier']) .load {
		background: transparent;
		color: var(--fg);
		border: 0;
		border-bottom: 2px solid var(--fg);
		border-radius: 0;
		padding: 10px 0;
	}
	:global(:root[data-appearance='atelier']) .balance-workspace {
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 36px;
	}
	:global(:root[data-appearance='atelier']) .erg-total strong {
		font-family: var(--font-display, Georgia);
		font-size: clamp(36px, 4vw, 64px);
	}
	:global(:root[data-appearance='atelier']) .token-balances {
		border: 0;
		border-left: var(--rule);
		border-radius: 0;
		background: transparent;
		padding: 0 0 0 28px;
	}
	:global(:root[data-appearance='atelier']) .member {
		grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr);
	}
	:global(:root[data-appearance='aurora']) .group-dashboard {
		padding: 36px;
		border-radius: 28px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .dashboard-head {
		display: block;
		text-align: center;
	}
	:global(:root[data-appearance='aurora']) .load {
		margin-top: 20px;
		border-radius: 100px;
		padding: 14px 24px;
	}
	:global(:root[data-appearance='aurora']) .disclosure {
		text-align: center;
		margin-inline: auto;
		max-width: 76ch;
	}
	:global(:root[data-appearance='aurora']) .balance-overview {
		padding: 32px;
		border: var(--rule);
		border-radius: 22px;
		background: var(--hero-bg);
		color: var(--hero-fg);
		text-align: center;
	}
	:global(:root[data-appearance='aurora']) .balance-overview .eyebrow,
	:global(:root[data-appearance='aurora']) .balance-overview dt,
	:global(:root[data-appearance='aurora']) .balance-overview dd span,
	:global(:root[data-appearance='aurora']) .balance-overview .raw-total,
	:global(:root[data-appearance='aurora']) .balance-overview .amount-note,
	:global(:root[data-appearance='aurora']) .erg-total > span {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='aurora']) .amount-note {
		margin-inline: auto;
	}
	:global(:root[data-appearance='aurora']) .erg-total strong {
		font-size: clamp(32px, 4.5vw, 62px);
	}
	:global(:root[data-appearance='aurora']) .token-balances {
		border-radius: 20px;
		padding: 24px;
	}
	:global(:root[data-appearance='aurora']) .members > ol {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 16px;
	}
	:global(:root[data-appearance='aurora']) .member {
		display: flex;
		flex-direction: column;
		padding: 22px;
		border: var(--rule);
		border-radius: 18px;
	}
	@media (max-width: 1100px) {
		:global(:root[data-appearance='prism']) .balance-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		.dashboard-head {
			flex-wrap: wrap;
		}
	}
	@media (max-width: 700px) {
		:global(:root[data-appearance='atelier']) .balance-workspace,
		:global(:root[data-appearance='aurora']) .members > ol {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .token-balances {
			border-left: 0;
			border-top: var(--rule);
			padding: 20px 0 0;
		}
	}
	@media (max-width: 450px) {
		.group-dashboard,
		:global(:root[data-appearance='prism']) .group-dashboard,
		:global(:root[data-appearance='aurora']) .group-dashboard {
			padding: 18px;
		}
		.member,
		:global(:root[data-appearance='atelier']) .member {
			grid-template-columns: minmax(0, 1fr);
		}
		.member-value {
			padding-left: 36px;
		}
		.metrics {
			gap: 12px;
			grid-template-columns: minmax(0, 1fr);
		}
		.metrics > div {
			display: flex;
			align-items: baseline;
			justify-content: space-between;
			gap: 12px;
		}
		dd {
			text-align: right;
			font-size: 18px;
		}
		:global(:root[data-appearance='aurora']) .balance-overview {
			padding: 24px 16px;
		}
		:global(:root[data-appearance='aurora']) .token-balances,
		.token-balances {
			padding: 16px;
		}
		.unloaded {
			align-items: start;
		}
		.load {
			width: 100%;
		}
	}
</style>
