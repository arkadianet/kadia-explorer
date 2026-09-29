<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Sparkline from '$lib/components/Sparkline.svelte';
	import Table from '$lib/components/Table.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import Age from '$lib/components/Age.svelte';
	import MinerChip from '$lib/components/MinerChip.svelte';
	import MempoolPreview from '$lib/components/MempoolPreview.svelte';
	import TokenBadge from '$lib/components/TokenBadge.svelte';
	import { relTime } from '$lib/format/time';
	import { formatErg } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import { api } from '$lib/api/endpoints';
	import { status as statusStore } from '$lib/status/status.svelte';
	import { appearance } from '$lib/theme/theme.svelte';
	import { formatHashrate, hashrateHs, TARGET_BLOCK_SECONDS } from '$lib/home/series';
	import type { TxSummaryDto } from '$lib/api/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// Prefer the layout's live-polled status once it has a value, so the countdown reflects the
	// same 5 s-fresh state as the header pill instead of going stale after the initial load.
	// The lag/stalled banner lives in +layout.svelte, so every route shows it.
	const status = $derived(statusStore.current ?? data.status.data);

	// `indexed`, not `best`: maturity heights come from the indexer, and the API's
	// `claimable_at_tip` is measured against the indexed tip too — using the node's `best`
	// would make the countdown disagree with every other page by the current lag.
	const tip = $derived(status?.indexed ?? null);
	const h = $derived(statusStore.health);

	const blocks = $derived(data.blocks.data?.recent_blocks ?? []);
	const latest = $derived(blocks[0] ?? null);
	const opening = $derived(
		{
			original: ['Transparent', 'by design.'],
			aurora: ['Every move.', 'In full light.'],
			atelier: ['Everything.', 'Accounted for.'],
			prism: ['Follow the chain.', 'In every dimension.']
		}[appearance.current]
	);

	let now = $state(Date.now());
	$effect(() => {
		const id = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(id);
	});

	const latestAge = $derived(latest ? relTime(latest.timestamp, now) : '—');

	// ---------------------------------------------------------------- windows and series
	const summary = $derived(data.blocks.data);
	const count = $derived(summary?.block_count ?? 0);
	const scope = $derived(`latest ${count.toLocaleString('en-US')} indexed blocks`);
	const dayNote = $derived(
		summary
			? `Heights ${summary.from_height ?? '\u2014'}\u2013${summary.to_height ?? '\u2014'}. Totals cover these blocks; charts place only this sample into timestamp buckets.`
			: 'Snapshot unavailable'
	);
	const dayTxs = $derived(summary?.transaction_count ?? 0);
	const txPerHour = $derived(summary?.transactions_per_hour ?? []);
	const blocksPerHour = $derived(summary?.blocks_per_hour ?? []);
	const totalFees = $derived(summary?.fees ?? '0');
	const roundedFees = $derived(BigInt(totalFees) % 1_000_000n !== 0n);
	const feesPerHour = $derived(
		(summary?.fees_per_hour ?? []).map((n) => Number(BigInt(n) / 1_000_000n) / 1000)
	);
	const heroSeries = $derived(summary?.blocks_per_ten_minutes ?? []);

	const hashrate = $derived(latest ? formatHashrate(hashrateHs(latest.difficulty)) : '—');

	const rentItems = $derived(data.rent.data?.items ?? []);
	const rentDue = $derived(rentItems.reduce((t, i) => t + BigInt(i.box.rent.due_nano), 0n));

	const recentBlocks = $derived(blocks.slice(0, 6));

	// ------------------------------------------------------------------ live transactions
	// The list follows the tip: when the status poll reports a new indexed height, the newest
	// transactions are pulled again. Nothing else on the page refetches, so a catching-up
	// indexer cannot turn the page into a request loop.
	// `null` until the first tip-driven refresh, so the server-loaded list stays the source
	// of truth for the first paint rather than being copied into state.
	let liveTxs = $state<TxSummaryDto[] | null>(null);
	let lastSeenTip = $state<number | null>(null);
	let initializedTip = false;
	let refreshing = false;

	$effect(() => {
		const t = statusStore.current?.indexed ?? null;
		if (t === null || t === lastSeenTip || refreshing) return;
		if (!initializedTip) {
			initializedTip = true;
			lastSeenTip = data.status.data?.indexed ?? t;
			if (t === lastSeenTip) return;
		}
		lastSeenTip = t;
		refreshing = true;
		void api
			.txSummaries(undefined, 6)
			.then((page) => (liveTxs = page.items))
			.catch(() => undefined)
			.finally(() => (refreshing = false));
	});

	const shownTxs = $derived((liveTxs ?? data.txs.data?.items ?? []).slice(0, 6));

	const tools = [
		{
			href: '/blocks',
			icon: 'blocks' as const,
			title: 'Browse blocks',
			sub: 'Every header, miner and reward'
		},
		{
			href: '/txs',
			icon: 'txs' as const,
			title: 'Follow transactions',
			sub: 'Inputs, outputs and the boxes between'
		},
		{
			href: '/rent',
			icon: 'rent-coin' as const,
			title: 'Analyse storage rent',
			sub: 'What is claimable, and what matures next'
		},
		{
			href: '/richlist',
			icon: 'richlist' as const,
			title: 'Trace an address',
			sub: 'Start from the largest holders'
		},
		{
			href: '/search',
			icon: 'search' as const,
			title: 'Search the chain',
			sub: 'A height, an id or an address'
		}
	];
</script>

<svelte:head>
	<title>Kadia — Ergo Explorer</title>
	<meta
		name="description"
		content="An Ergo blockchain explorer: blocks, transactions, boxes, addresses and storage rent, read straight from an indexed node."
	/>
</svelte:head>

<div class="home-dashboard">
	<!-- ------------------------------------------------------------------------------- hero -->
	<section class="hero">
		<div class="hero-in">
			<div class="hero-copy">
				<p class="kicker">KADIA / ERGO MAINNET</p>
				<h1>{opening[0]}<br /><span>{opening[1]}</span></h1>
				<p class="lede">Explore, understand, and build on Ergo.</p>
				<div class="cta">
					<a class="btn btn-fill" href="/blocks">
						Explore blocks
						<Icon name="arrow-right" size={18} />
					</a>
					<a class="btn btn-ghost" href="/rent">
						Storage rent
						<Icon name="arrow-right" size={18} />
					</a>
				</div>
			</div>

			{#if latest}
				<div class="tipcard">
					<div class="tipcard-head">
						<Icon name="box" size={20} />
						<span>Snapshot indexed height</span>
					</div>
					<p class="tipcard-height">{latest.height.toLocaleString('en-US')}</p>
					<p class="tipcard-age">{latestAge}</p>
					<Sparkline
						values={heroSeries}
						kind="bars"
						color="var(--hero-chart, var(--accent))"
						height={40}
						title="Blocks per 10-minute bucket over the six hours of chain below the indexed tip, counted from the loaded block timestamps."
					/>
					<p
						class="tipcard-foot"
						title={`Difficulty ${latest.difficulty} divided by Ergo's ${TARGET_BLOCK_SECONDS} s target block time — the hashrate that would produce this difficulty on average.`}
					>
						<span>blocks per 10 min, 6 h</span>
						<b>{hashrate}</b>
					</p>
				</div>
			{/if}
			<div class="home-chain-scene" aria-label="Recent indexed blocks">
				{#each recentBlocks.slice(0, 3) as block, index (block.id)}
					<a class="scene-block" href={`/blocks/${block.height}`} style={`--scene-order: ${index}`}>
						<span class="scene-index">Indexed block / 0{index + 1}</span>
						<strong class="scene-height">{block.height.toLocaleString('en-US')}</strong>
						<span class="scene-count"
							>{block.tx_count} transaction{block.tx_count === 1 ? '' : 's'}</span
						>
						<span class="scene-id mono">{truncateMiddle(block.id, 8, 6)}</span>
						<Icon name="arrow-right" size={18} />
					</a>
				{/each}
			</div>
		</div>
	</section>

	<!-- ------------------------------------------------------------------------------ stats -->
	<section class="stats" aria-label="Recent indexed chain">
		<div class="stat glass">
			<p class="stat-label"><Icon name="txs" size={16} />Transactions</p>
			<p
				class="stat-value"
				title={`Sum of tx_count over the ${count} indexed blocks in this window. ${dayNote}`}
			>
				{data.blocks.error ? 'Unavailable' : dayTxs.toLocaleString('en-US')}
			</p>
			<Sparkline
				values={txPerHour}
				color="var(--accent-ink)"
				kind="bars"
				title="Transactions from the sampled blocks, placed in hourly buckets below the tip timestamp. Uncovered hours are not proof of no activity."
			/>
			<p class="stat-foot">
				{data.blocks.error ? 'Window unavailable' : scope}
			</p>
		</div>

		<div class="stat glass">
			<p class="stat-label"><Icon name="blocks" size={16} />Blocks</p>
			<p class="stat-value" title={`Exact count in the bounded block sample. ${dayNote}`}>
				{data.blocks.error ? 'Unavailable' : count.toLocaleString('en-US')}
			</p>
			<Sparkline
				values={blocksPerHour}
				color="var(--accent-ink)"
				kind="line"
				title="Sampled blocks placed in hourly buckets below the tip timestamp."
			/>
			<p class="stat-foot" title="Ergo targets one block every 120 seconds.">
				{scope}
			</p>
		</div>

		<div class="stat glass">
			<p class="stat-label"><Icon name="spark" size={16} />Transaction fees</p>
			<p
				class="stat-value"
				title={`Exact transaction fees: ${totalFees} nanoERG. Displayed to three decimal places. ${dayNote}`}
			>
				{data.blocks.error
					? 'Unavailable'
					: `${roundedFees ? '≈ ' : ''}${formatErg(totalFees, { maxFrac: 3 })}`}<span class="unit"
					>ERG</span
				>
			</p>
			<Sparkline
				values={feesPerHour}
				color="var(--accent-ink)"
				kind="line"
				title="Transaction fees from sampled blocks, placed in hourly buckets below the tip timestamp."
			/>
			<p class="stat-foot">{scope}</p>
		</div>

		<div class="stat glass">
			<p class="stat-label"><Icon name="rent-coin" size={16} />Storage rent</p>
			<p
				class="stat-value"
				title="Boxes whose storage-rent maturity falls within the next 720 blocks, from /v1/rent/upcoming."
			>
				{data.rent.error
					? 'Unavailable'
					: `${data.rent.data?.complete === true ? '' : '≥ '}${rentItems.length.toLocaleString('en-US')}`}
			</p>
			<p class="stat-sub">
				{#if data.rent.error}Unavailable{:else}{data.rent.data?.complete === true
						? ''
						: '≥ '}<Amount nano={rentDue.toString()} maxFrac={3} /> due{/if}
			</p>
			<p class="stat-foot">
				maturing in 720 blocks{data.rent.data?.complete === true ? '' : ' · incomplete'}
			</p>
		</div>

		<div class="stat glass">
			<p class="stat-label"><Icon name="status" size={16} />Indexer</p>
			<p class="stat-value tone-{h.tone}" title={h.detail}>{h.label}</p>
			<p class="stat-sub">
				{status ? `${status.lag_blocks.toLocaleString('en-US')} blocks behind` : '—'}
			</p>
			<p class="stat-foot">{status ? `${status.mode} mode` : ''}</p>
		</div>
	</section>

	<MempoolPreview />

	<!-- ----------------------------------------------------------------------------- panels -->
	{#if summary && count > 0}
		<p class="window-caption">
			Network sample: heights {summary.from_height?.toLocaleString(
				'en-US'
			)}–{summary.to_height?.toLocaleString('en-US')}. Hourly charts cover these blocks only.
			{#if summary.partial_from !== null}Indexed history begins at block {summary.partial_from.toLocaleString(
					'en-US'
				)}.{/if}
		</p>
	{/if}
	<div class="panels">
		<section class="panel card">
			<div class="card-head">
				<h2 class="card-title">Recent blocks</h2>
				<a class="more" href="/blocks">View all<Icon name="chevron-right" size={14} /></a>
			</div>
			{#if data.blocks.error}
				<div class="pad"><ErrorState error={data.blocks.error} /></div>
			{:else if recentBlocks.length === 0}
				<div class="pad">
					<EmptyState message="Blocks will appear here as the indexer catches up with the node." />
				</div>
			{:else}
				<Table>
					{#snippet head()}
						<tr>
							<th>Height</th>
							<th>Age</th>
							<th class="num">Txs</th>
							<th class="num">Fees</th>
							<th>Miner</th>
						</tr>
					{/snippet}
					{#each recentBlocks as block (block.id)}
						<tr>
							<td><a class="height" href={`/blocks/${block.height}`}>{block.height}</a></td>
							<td class="muted"><Age ms={block.timestamp} /></td>
							<td class="num mono">{block.tx_count}</td>
							<td class="num"><Amount nano={block.fees} maxFrac={3} /></td>
							<td><MinerChip minerPk={block.miner_pk} /></td>
						</tr>
					{/each}
				</Table>
			{/if}
		</section>

		<section class="panel live ink">
			<div class="card-head">
				<h2 class="card-title">Live transactions</h2>
				<span class="pill live-pill tone-{h.tone}"><span class="dot"></span>{h.live}</span>
			</div>
			{#if data.txs.error && liveTxs === null}
				<div class="pad"><ErrorState error={data.txs.error} /></div>
			{:else if shownTxs.length === 0}
				<div class="pad">
					<EmptyState message="Transactions will appear here as the indexer catches up." />
				</div>
			{:else}
				<ul class="txlist">
					{#each shownTxs as tx (tx.id)}
						<li>
							<a href={`/tx/${tx.id}`}>
								<span class="kind"><Icon name="txs" size={18} /></span>
								<span class="tx-main">
									<span class="tx-kind">{tx.input_count} in · {tx.output_count} out</span>
									<span class="tx-id mono">{truncateMiddle(tx.id)}</span>
								</span>
								<span class="tx-age"><Age ms={tx.timestamp} /></span>
								<span class="tx-value">
									{formatErg(tx.fee, { maxFrac: 4 })}<span class="unit">ERG fee</span>
								</span>
							</a>
						</li>
					{/each}
				</ul>
				<div class="live-foot">
					<span
						title={`Sum of tx_count over the ${count} indexed blocks in this window. ${dayNote}`}
					>
						{data.blocks.error ? 'Unavailable' : dayTxs.toLocaleString('en-US')} transactions · {scope}
					</span>
					<span class="live-spark">
						<Sparkline
							values={txPerHour}
							kind="bars"
							height={26}
							color="var(--accent)"
							title="Transactions from the sampled blocks, placed in hourly buckets below the tip timestamp. Uncovered hours are not proof of no activity."
						/>
					</span>
				</div>
			{/if}
		</section>
	</div>

	<!-- ----------------------------------------------------------- rent, tokens and holders -->
	<div class="panels three">
		<section class="panel card">
			<div class="card-head">
				<h2 class="card-title">Rent maturing soon</h2>
				<a class="more" href="/rent">All rent<Icon name="chevron-right" size={14} /></a>
			</div>
			{#if data.rent.error}
				<div class="pad"><ErrorState error={data.rent.error} /></div>
			{:else if rentItems.length === 0}
				<div class="pad">
					<EmptyState
						message="No upcoming boxes were returned for this indexed snapshot. Coverage may be limited."
					/>
				</div>
			{:else}
				<p class="summary">
					<b
						>{data.rent.error
							? 'Unavailable'
							: `${data.rent.data?.complete === true ? '' : '≥ '}${rentItems.length.toLocaleString('en-US')}`}</b
					>
					boxes mature in the next 720 blocks, owing at least
					<b><Amount nano={rentDue.toString()} maxFrac={3} /></b> between them.
				</p>
				<Table dense>
					{#snippet head()}
						<tr>
							<th class="num">Value</th>
							<th class="num">Due</th>
							<th>Matures in</th>
							<th class="rent-addr">Address</th>
						</tr>
					{/snippet}
					{#each rentItems.slice(0, 5) as item (item.box.id)}
						<tr>
							<!-- The value is the row's link to the box it belongs to: without it a rent row
						     names an amount and nothing that carries it. -->
							<td class="num">
								<a href={`/box/${item.box.id}`} title={item.box.id}>
									<Amount nano={item.box.value} maxFrac={3} />
								</a>
							</td>
							<td class="num"><Amount nano={item.box.rent.due_nano} maxFrac={3} /></td>
							<td class="mono muted">
								{tip !== null
									? `${item.maturity_height - tip} blocks`
									: `at ${item.maturity_height}`}
							</td>
							<td class="rent-addr">
								{#if item.box.address}
									<Hash
										value={item.box.address}
										href={`/address/${item.box.address}`}
										copy={false}
									/>
								{:else}
									<span class="muted" title="No P2PK/P2S address for this box">—</span>
								{/if}
							</td>
						</tr>
					{/each}
				</Table>
			{/if}
		</section>

		<section class="panel card">
			<div class="card-head">
				<h2 class="card-title">Top tokens</h2>
				<a class="more" href="/tokens">All tokens<Icon name="chevron-right" size={14} /></a>
			</div>
			{#if data.tokens.error}
				<div class="pad"><ErrorState error={data.tokens.error} /></div>
			{:else if (data.tokens.data ?? []).length === 0}
				<div class="pad">
					<EmptyState
						message="No tokens indexed yet — they appear as the indexer reaches the blocks that mint them."
					/>
				</div>
			{:else}
				<ol class="tokens">
					{#each data.tokens.data ?? [] as token, i (token.id)}
						<li>
							<span class="rank">{i + 1}</span>
							{#if token.name.trim()}
								<a class="token-name" href={`/token/${token.id}`} title={token.name.trim()}>
									{token.name.trim()}
								</a>
							{:else}
								<!-- No minted name: the id stands in, in the mono face ids always take, and
							     short enough that the cell never ellipsises an already-elided hash. -->
								<a class="mono token-id" href={`/token/${token.id}`} title={token.id}>
									{truncateMiddle(token.id, 4, 4)}
								</a>
							{/if}
							<TokenBadge kind={token.kind} />
							<span class="token-holders" title="Addresses holding this token">
								{token.holder_count.toLocaleString('en-US')}<span class="unit">holders</span>
							</span>
						</li>
					{/each}
				</ol>
			{/if}
		</section>

		<section class="panel card">
			<div class="card-head">
				<h2 class="card-title">Largest holders</h2>
				<a class="more" href="/richlist">Rich list<Icon name="chevron-right" size={14} /></a>
			</div>
			{#if data.richlist.error}
				<div class="pad"><ErrorState error={data.richlist.error} /></div>
			{:else if (data.richlist.data ?? []).length === 0}
				<div class="pad"><EmptyState message="The richlist is still being built." /></div>
			{:else}
				<ol class="holders">
					{#each data.richlist.data ?? [] as item, i (item.tree_hash)}
						<li>
							<span class="rank">{i + 1}</span>
							{#if item.address}
								<a class="mono holder-id" href={`/address/${item.address}`}>
									{truncateMiddle(item.address)}
								</a>
							{:else}
								<span class="mono holder-id" title={item.tree_hash}>
									{truncateMiddle(item.tree_hash)}
								</span>
							{/if}
							<span class="holder-bal"><Amount nano={item.nano} maxFrac={0} /></span>
						</li>
					{/each}
				</ol>
			{/if}
		</section>
	</div>

	<!-- -------------------------------------------------------------------------- go deeper -->
	<section class="deeper">
		<div class="deeper-in">
			<div class="deeper-head">
				<h2>Go deeper</h2>
				<p>Five ways into the same chain, depending on what you already know.</p>
			</div>
			<div class="tools">
				{#each tools as tool (tool.href)}
					<a class="tool" href={tool.href}>
						<span class="tool-icon"><Icon name={tool.icon} size={22} /></span>
						<span class="tool-title">{tool.title}</span>
						<span class="tool-sub">{tool.sub}</span>
					</a>
				{/each}
			</div>
		</div>
	</section>
</div>

<style>
	.window-caption {
		color: var(--fg-muted);
		font-size: var(--fs-micro);
		margin: -12px 0 20px;
		line-height: 1.6;
	}
	.home-dashboard {
		display: contents;
	}
	.home-chain-scene {
		display: none;
	}
	/* Full-bleed sections cancel the content column's gutter and, for the hero, its top
	   padding as well — the landscape has to run under the floating header. */
	.hero,
	.deeper {
		position: relative;
		isolation: isolate;
		margin-inline: calc(var(--gutter) * -1);
		overflow: hidden;
	}

	.hero {
		margin: 8px 0 0;
		border-radius: 20px;
		background: var(--hero-bg, #103a28);
		color: var(--hero-fg, #eef8e8);
	}

	.hero-in {
		position: relative;
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: var(--space-8);
		padding: 36px;
		min-height: 280px;
	}

	.hero-copy {
		max-width: 480px;
	}

	/* The one loud element on the page: a light, very large display line. Nothing else here
	   competes with it, which is why the buttons underneath are small and quiet. */
	h1 {
		font-size: var(--fs-display);
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 800);
		line-height: 1.02;
		letter-spacing: -0.028em;
		color: var(--hero-fg, #e7ffc7);
	}

	.lede {
		margin-top: var(--space-4);
		font-size: 18px;
		color: var(--hero-muted, #c9dccd);
	}

	:global(:root[data-theme='dark']) .hero h1 {
		color: var(--hero-fg, #f2f6f3);
	}

	:global(:root[data-theme='dark']) .hero .lede {
		color: var(--hero-muted, #c3d0c8);
	}

	.cta {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		margin-top: var(--space-6);
	}

	/* The tip card: the single number a returning visitor came for, floating over the valley. */
	.tipcard {
		flex: none;
		width: 250px;
		/* The stat row rises 68 px into the hero; this keeps a clear 28 px of sky between the
		   card's bottom edge and the top of those cards at every desktop width. */
		margin-bottom: 0;
		padding: var(--space-4) var(--space-5) var(--space-5);
		border-radius: var(--radius-card);
		background: var(--hero-tip-bg, #071f16);
		backdrop-filter: blur(16px) saturate(1.2);
		-webkit-backdrop-filter: blur(16px) saturate(1.2);
		border: 1px solid var(--hero-hairline, rgba(233, 238, 234, 0.16));
		box-shadow: var(--shadow-lift);
		color: var(--hero-tip-fg, var(--ink-fg));
	}

	.tipcard-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--fs-micro);
		font-weight: 600;
		color: var(--hero-tip-muted, var(--ink-fg-muted));
	}

	.tipcard-height {
		margin-top: var(--space-2);
		font-size: var(--fs-key);
		font-family: var(--font-number, var(--font-sans));
		font-weight: var(--weight-number, 700);
		letter-spacing: -0.03em;
		line-height: 1.1;
	}

	.tipcard-age {
		font-size: var(--fs-micro);
		color: var(--hero-tip-muted, var(--ink-fg-muted));
		margin-bottom: var(--space-3);
	}

	.tipcard-foot {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		margin-top: var(--space-2);
		font-size: var(--fs-micro);
		color: var(--hero-tip-muted, var(--ink-fg-muted));
	}

	.tipcard-foot b {
		color: var(--hero-tip-fg, var(--ink-fg));
		font-weight: 600;
	}

	/* ---------------------------------------------------------------------------- stats */
	.stats {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: var(--space-6);
		margin-top: 0;
		position: relative;
		z-index: 5;
	}

	.stat {
		padding: var(--space-4) var(--space-4) var(--space-3);
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.stat-label {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: var(--fs-micro);
		font-weight: 600;
		color: var(--fg-muted);
	}

	.stat-value {
		font-size: 26px;
		font-family: var(--font-number, var(--font-sans));
		font-weight: var(--weight-number, 600);
		letter-spacing: -0.03em;
		line-height: 1.2;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.stat-value .unit {
		font-size: 13px;
		font-weight: 500;
		color: var(--fg-muted);
		margin-left: 4px;
	}

	.stat-sub {
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}

	.stat-foot {
		margin-top: auto;
		padding-top: var(--space-2);
		font-size: 11.5px;
		color: var(--fg-muted);
	}

	.tone-ok {
		color: var(--ok-ink);
	}
	.tone-warn {
		color: var(--warn-ink);
	}
	.tone-danger {
		color: var(--danger-ink);
	}

	/* --------------------------------------------------------------------------- panels */
	.panels {
		display: grid;
		grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr);
		gap: var(--space-5);
		align-items: start;
	}

	/* Three summaries of "what is on the chain right now", side by side. The rent card holds a
	   table and the other two hold lists, so it keeps a little more width. */
	.panels.three {
		grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr) minmax(0, 1fr);
	}

	.panel {
		overflow: hidden;
		min-width: 0;
	}

	.pad {
		padding: var(--space-6) var(--space-5);
	}

	.summary {
		padding: var(--space-4) var(--space-5);
		font-size: var(--fs-data);
		color: var(--fg-muted);
		border-bottom: var(--rule);
	}

	.summary b {
		color: var(--fg);
		font-weight: 600;
	}

	.height {
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}

	.muted {
		color: var(--fg-muted);
	}

	/* ------------------------------------------------------------- live transaction list */
	.live .card-head {
		border-bottom-color: var(--ink-hairline);
	}

	.live-pill {
		height: 26px;
		background: rgba(233, 238, 234, 0.1);
		color: var(--ink-fg-muted);
	}

	.live-pill.tone-ok {
		background: rgba(31, 181, 107, 0.16);
		color: #6fdca7;
	}

	.live-pill.tone-warn {
		background: rgba(201, 138, 0, 0.18);
		color: #e8be6a;
	}

	.live-pill.tone-danger {
		background: rgba(210, 75, 75, 0.18);
		color: #f0a0a0;
	}

	.txlist li + li {
		border-top: 1px solid var(--ink-hairline);
	}

	.txlist a {
		display: grid;
		grid-template-columns: 34px minmax(0, 1fr) auto auto;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-5);
	}

	.txlist a:hover {
		background: rgba(233, 238, 234, 0.05);
		color: var(--ink-fg);
	}

	.kind {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		border-radius: 10px;
		background: rgba(233, 238, 234, 0.08);
		color: var(--ink-fg-muted);
	}

	.kind.rent {
		background: rgba(31, 181, 107, 0.16);
		color: #6fdca7;
	}

	.kind.token {
		background: rgba(201, 138, 0, 0.18);
		color: #e8be6a;
	}

	.tx-main {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.tx-kind {
		font-size: var(--fs-data);
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.tx-id {
		font-size: 11.5px;
		color: var(--ink-fg-muted);
	}

	.tx-age {
		font-size: var(--fs-micro);
		color: var(--ink-fg-muted);
		white-space: nowrap;
	}

	.tx-value {
		font-size: var(--fs-data);
		font-weight: 600;
		white-space: nowrap;
	}

	.tx-value .unit {
		font-size: 10.5px;
		font-weight: 500;
		color: var(--ink-fg-muted);
		margin-left: 3px;
	}

	.live-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-3) var(--space-5);
		border-top: 1px solid var(--ink-hairline);
		font-size: var(--fs-micro);
		color: var(--ink-fg-muted);
	}

	.live-spark {
		width: 96px;
		flex: none;
	}

	/* --------------------------------------------------------------------- top tokens */
	/* The same row shape as the holders list beside it — rank, name, figure — with the kind
	   pill in between, since a token's name alone does not say whether it is an NFT. */
	.tokens li {
		display: grid;
		grid-template-columns: 22px minmax(0, 1fr) auto auto;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-5);
	}

	.tokens li + li {
		border-top: var(--rule);
	}

	.token-name {
		font-size: var(--fs-data);
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.token-id {
		font-size: var(--fs-data);
		white-space: nowrap;
	}

	.token-holders {
		font-size: var(--fs-data);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	/* The figure needs its noun: unlike the ERG balances beside it, a bare count says nothing
	   about what was counted. */
	.token-holders .unit {
		font-size: 10.5px;
		font-weight: 500;
		color: var(--fg-muted);
		margin-left: 4px;
	}

	/* The address is the first thing to go when the rent card is one of three across a
	   desktop row: three narrow columns of figures still read, four do not. Below the
	   breakpoint the card is full width again and the column comes back. */
	@media (min-width: 1181px) {
		.panels.three .rent-addr {
			display: none;
		}
	}

	/* -------------------------------------------------------------------------- holders */
	.holders li {
		display: grid;
		grid-template-columns: 22px minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-5);
	}

	.holders li + li {
		border-top: var(--rule);
	}

	.rank {
		font-size: var(--fs-micro);
		font-weight: 600;
		color: var(--fg-muted);
	}

	.holder-id {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.holder-bal {
		font-size: var(--fs-data);
		font-weight: 600;
	}

	/* ------------------------------------------------------------------------ go deeper */
	.deeper {
		border-radius: var(--radius-card);
		margin-inline: calc(var(--gutter) * -1);
	}

	.deeper-in {
		position: relative;
		padding: var(--space-10) var(--gutter) var(--space-12);
		color: var(--ink-fg);
	}

	.deeper-head h2 {
		font-size: 34px;
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 300);
		letter-spacing: -0.025em;
	}

	.deeper-head p {
		margin-top: var(--space-2);
		font-size: var(--fs-body);
		color: var(--ink-fg-muted);
		max-width: 46ch;
	}

	.tools {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: var(--space-4);
		margin-top: var(--space-8);
	}

	.tool {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: var(--space-5) var(--space-4);
		border-radius: var(--radius-card);
		background: var(--tool-bg, rgba(12, 20, 15, 0.55));
		backdrop-filter: blur(12px);
		-webkit-backdrop-filter: blur(12px);
		border: 1px solid rgba(233, 238, 234, 0.13);
		color: var(--ink-fg);
		min-height: 148px;
	}

	.tool:hover,
	.tool:focus-visible {
		color: var(--ink-fg);
		border-color: var(--accent);
		background: var(--tool-hover, rgba(12, 20, 15, 0.72));
	}

	.tool-icon {
		color: var(--accent);
		margin-bottom: var(--space-3);
	}

	.tool-title {
		font-size: var(--fs-body);
		font-weight: 600;
		letter-spacing: -0.01em;
	}

	.tool-sub {
		font-size: var(--fs-micro);
		line-height: 1.45;
		color: var(--ink-fg-muted);
	}

	/* ------------------------------------------------------------------------ responsive */
	@media (max-width: 1180px) {
		.stats {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
		.tools {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
	}

	@media (max-width: 1180px) {
		/* Three cards in a row need the full desktop width; below it they would each be too
		   narrow for the rent table, so the row becomes a single column rather than an
		   awkward two-plus-one. */
		.panels.three {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	@media (max-width: 980px) {
		.panels {
			grid-template-columns: minmax(0, 1fr);
		}
		/* Stacked, the card is no longer beside the copy and the stat row no longer climbs
		   into the hero, so neither offset applies. */
		.tipcard {
			margin-bottom: 0;
		}
		.stats {
			margin-top: var(--space-2);
		}
		.hero-in {
			flex-direction: column;
			align-items: stretch;
			gap: var(--space-8);
			padding: 24px;
			min-height: 0;
		}
		.tipcard {
			width: 100%;
		}
	}

	@media (max-width: 700px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.tools {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.hero {
			border-radius: 16px;
		}
	}
	.kicker {
		font: 11px var(--font-mono);
		letter-spacing: 0.12em;
		color: var(--hero-highlight, #b7f25f);
		margin-bottom: 18px;
	}
	.hero .btn-fill {
		background: var(--hero-btn-fill, #b7f25f);
		color: var(--hero-btn-fg, #10291e);
	}
	.hero :global(:focus-visible) {
		outline-color: var(--hero-focus, #b7f25f);
	}
	.deeper :global(:focus-visible) {
		outline-color: var(--accent);
	}
	.hero .btn-ghost {
		background: transparent;
		color: var(--hero-fg, #eef8e8);
		border-color: var(--hero-hairline, #6a8a73);
	}
	.deeper {
		background: var(--ink-panel);
		border-top: 3px solid var(--accent);
	}
</style>
