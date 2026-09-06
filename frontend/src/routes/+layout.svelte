<script lang="ts">
	import '$lib/styles/tokens.css';
	import '$lib/styles/base.css';
	import { page } from '$app/state';
	import { theme } from '$lib/theme/theme.svelte';
	import { status } from '$lib/status/status.svelte';
	import { health } from '$lib/status/health';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import SearchBox from '$lib/components/SearchBox.svelte';
	import Icon, { type IconName } from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	// The lag/stalled banner lives here rather than on the home page so every route surfaces a
	// degraded index — the status store is the single source, polled every 5 s by `status.start()`.
	const stalledInfo = $derived(status.current?.stalled ?? null);
	const showLagBanner = $derived.by(() => {
		const s = status.current;
		return s !== null && (s.lag_blocks > 100 || s.halted !== null);
	});

	const h = $derived(health(status.current));

	/** The header floats over the page's first section and the lag banner floats under it, so
	 * both have to be reserved as space: `--banner-h` on the main column is what every page
	 * (and the home hero, which bleeds back up through it) offsets itself by. */
	const hasBanner = $derived(stalledInfo !== null || (showLagBanner && status.current !== null));

	interface NavLink {
		href: string;
		label: string;
		icon: IconName;
		/** Whether the item earns a slot in the phone's bottom bar (five fit; six do not). */
		primary?: boolean;
	}

	const navLinks: NavLink[] = [
		{ href: '/', label: 'Home', icon: 'home', primary: true },
		{ href: '/blocks', label: 'Blocks', icon: 'blocks', primary: true },
		{ href: '/txs', label: 'Transactions', icon: 'txs', primary: true },
		{ href: '/richlist', label: 'Rich list', icon: 'richlist' },
		{ href: '/rent', label: 'Storage rent', icon: 'rent-coin', primary: true },
		{ href: '/status', label: 'Status', icon: 'status', primary: true }
	];

	function isActive(href: string): boolean {
		const path = page.url.pathname;
		if (href === '/') return path === '/';
		return path === href || path.startsWith(`${href}/`);
	}

	// A transaction detail page belongs to the Transactions section. Boxes and addresses have
	// no section of their own, so they light nothing rather than claiming somebody else's row.
	const SECTION_OF: Record<string, string> = { '/tx': '/txs' };

	function isCurrent(href: string): boolean {
		if (isActive(href)) return true;
		const first = `/${page.url.pathname.split('/')[1] ?? ''}`;
		return SECTION_OF[first] === href;
	}

	const year = new Date().getFullYear();

	onMount(() => {
		theme.init();
		status.start();
		return () => status.stop();
	});
</script>

<div class="shell">
	<aside class="rail">
		<div class="rail-in">
			<a class="brand" href="/">
				<span class="mark" aria-hidden="true">Σ</span>
				<span class="wordmark">
					<span class="name">Kadia</span>
					<span class="sub">Ergo Explorer</span>
				</span>
			</a>

			<nav class="nav" aria-label="Primary">
				<ul>
					{#each navLinks as link (link.href)}
						<li>
							<a
								href={link.href}
								class:current={isCurrent(link.href)}
								aria-current={isActive(link.href) ? 'page' : undefined}
							>
								<Icon name={link.icon} size={19} />
								{link.label}
							</a>
						</li>
					{/each}
				</ul>
			</nav>

			<div class="rail-foot">
				<!-- The chain's state, restated where the eye rests between navigations. Every figure
			     comes from /v1/status; nothing here is estimated. -->
				<a class="chain" href="/status" title={h.detail}>
					<span class="chain-head">
						<span class="dot tone-{h.tone}" aria-hidden="true"></span>
						Ergo mainnet
					</span>
					<span class="chain-row">
						<span>Indexed height</span>
						<b>{status.current?.indexed?.toLocaleString('en-US') ?? '—'}</b>
					</span>
					<span class="chain-row">
						<span>Behind the node</span>
						<b>{status.current ? `${status.current.lag_blocks.toLocaleString('en-US')}` : '—'}</b>
					</span>
					<span class="chain-row">
						<span>Indexer</span>
						<b>{status.current ? `${h.label.toLowerCase()}, ${status.current.mode}` : '—'}</b>
					</span>
				</a>

				<p class="note">
					Every figure here is read from the chain or computed from it. Hover one to see how.
				</p>
			</div>
		</div>
	</aside>

	<div class="main" style:--banner-h={hasBanner ? '30px' : '0px'}>
		<header class="topbar">
			<div class="search-slot" data-testid="search-slot">
				<SearchBox />
			</div>
			<div class="status-slot" data-testid="status-slot">
				<StatusBadge />
			</div>
			<button
				type="button"
				class="theme-toggle"
				aria-label="Toggle theme"
				onclick={() => theme.toggle()}
			>
				<Icon name={theme.current === 'dark' ? 'sun' : 'moon'} size={18} />
			</button>
		</header>

		{#if stalledInfo}
			<div class="banner danger" role="alert">
				<span class="dot" aria-hidden="true"></span>
				<span
					>Indexer is waiting on block {stalledInfo.height} from the node for {stalledInfo.since_secs}
					s.</span
				>
			</div>
		{:else if showLagBanner && status.current}
			<div class="banner warn" role="status">
				<span class="dot" aria-hidden="true"></span>
				<span>Index is {status.current.lag_blocks} blocks behind the node.</span>
			</div>
		{/if}

		<main class="content">
			{@render children()}
		</main>

		<footer class="foot">
			<div class="foot-in">
				<a class="brand small" href="/">
					<span class="mark" aria-hidden="true">Σ</span>
					<span class="wordmark"><span class="name">Kadia</span></span>
				</a>
				<p class="foot-line">Open source, built for a fairer and more open future.</p>
				<nav class="foot-links" aria-label="Footer">
					<a href="/v1/status">API</a>
					<a href="/status">Status</a>
					<a href="https://github.com/arkadianet" rel="noreferrer">GitHub</a>
				</nav>
				<span class="pill mainnet"><span class="dot tone-{h.tone}"></span>Ergo mainnet</span>
			</div>
			<p class="copy">© {year} Kadia</p>
		</footer>
	</div>
</div>

<nav class="tabbar" aria-label="Primary">
	{#each navLinks.filter((l) => l.primary) as link (link.href)}
		<a
			href={link.href}
			class:current={isCurrent(link.href)}
			aria-current={isActive(link.href) ? 'page' : undefined}
		>
			<Icon name={link.icon} size={20} />
			<span
				>{link.label === 'Transactions'
					? 'Txs'
					: link.label === 'Storage rent'
						? 'Rent'
						: link.label}</span
			>
		</a>
	{/each}
</nav>

<style>
	.shell {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		min-height: 100vh;
	}

	/* ------------------------------------------------------------------------- the rail */
	/* The dark column paints the whole grid row; only its contents stick, so the panel never
	   stops halfway down a long page. */
	.rail {
		background: var(--ink-panel);
		color: var(--ink-fg);
	}

	.rail-in {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		padding: var(--space-5) var(--space-4);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: 0 var(--space-2);
	}

	.brand:hover,
	.brand:focus-visible {
		color: inherit;
	}

	.mark {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		flex: none;
		border-radius: 11px;
		background: var(--ink-fg);
		color: var(--ink-panel);
		font-size: 19px;
		font-weight: 700;
		line-height: 1;
	}

	.wordmark {
		display: flex;
		flex-direction: column;
		line-height: 1.15;
		min-width: 0;
	}

	.name {
		font-size: var(--fs-title);
		font-weight: 700;
		letter-spacing: -0.02em;
	}

	.sub {
		font-size: 11px;
		font-weight: 500;
		color: var(--ink-fg-muted);
	}

	.nav ul {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.nav a {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		height: 42px;
		padding: 0 var(--space-3);
		border-radius: var(--radius-control);
		font-size: var(--fs-body);
		font-weight: 500;
		color: var(--ink-fg-muted);
	}

	.nav a:hover,
	.nav a:focus-visible {
		color: var(--ink-fg);
		background: rgba(233, 238, 234, 0.06);
	}

	/* The active item is a filled pill, the only light shape in a dark column. */
	.nav a.current {
		color: var(--ink-panel);
		background: var(--ink-fg);
		font-weight: 600;
	}

	.rail-foot {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.chain {
		display: flex;
		flex-direction: column;
		gap: 7px;
		padding: var(--space-4);
		border-radius: var(--radius-card);
		background: rgba(233, 238, 234, 0.05);
		border: 1px solid var(--ink-hairline);
		font-size: var(--fs-micro);
		color: var(--ink-fg-muted);
	}

	.chain:hover,
	.chain:focus-visible {
		color: var(--ink-fg-muted);
		border-color: rgba(233, 238, 234, 0.24);
	}

	.chain-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--ink-fg);
		font-size: var(--fs-label);
		font-weight: 600;
		margin-bottom: 2px;
	}

	.chain-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.chain-row b {
		color: var(--ink-fg);
		font-weight: 600;
	}

	.tone-ok {
		color: var(--accent);
	}
	.tone-warn {
		color: var(--warn);
	}
	.tone-danger {
		color: var(--danger);
	}
	.tone-neutral {
		color: var(--ink-fg-muted);
	}

	.note {
		padding: 0 var(--space-2);
		font-size: 11.5px;
		line-height: 1.5;
		color: var(--ink-fg-muted);
	}

	/* ------------------------------------------------------------------------ the main column */
	.main {
		display: flex;
		flex-direction: column;
		min-width: 0;
		position: relative;
	}

	/* The header floats over whatever the page opens with — on the home page that is the
	   landscape, so the search pill sits on the sky rather than on a bar of its own. */
	.topbar {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		z-index: 30;
		display: flex;
		align-items: center;
		gap: var(--space-3);
		height: var(--topbar-h);
		padding: 0 var(--gutter);
	}

	.search-slot {
		flex: 1;
		min-width: 0;
		max-width: 560px;
	}

	.status-slot {
		margin-left: auto;
		flex: none;
	}

	.theme-toggle {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		flex: none;
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: 50%;
		color: var(--fg-muted);
		box-shadow: var(--shadow-hair);
		cursor: pointer;
	}

	.theme-toggle:hover {
		color: var(--fg);
		border-color: var(--fg-muted);
	}

	/* A caveat, not an alarm: one line of tone-coloured text with a dot, sitting on the hero's
	   top edge. A boxed amber panel here reads as an error dialogue dropped onto the page and
	   breaks the opening composition, so this carries no background, border or shadow. */
	.banner {
		position: absolute;
		top: calc(var(--topbar-h) - var(--space-2));
		left: var(--gutter);
		right: var(--gutter);
		z-index: 25;
		max-width: 1240px;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		height: 26px;
		font-size: var(--fs-data);
		font-weight: 600;
		/* Text on the hero's sky and, deeper in the page, on a card; a shadow the width of a
		   hairline keeps it legible over the lightest part of the landscape without drawing a
		   box around it. */
		text-shadow: 0 1px 0 rgba(255, 255, 255, 0.5);
	}

	.banner.warn {
		color: var(--warn-ink);
	}

	.banner.danger {
		color: var(--danger-ink);
	}

	.banner .dot {
		width: 6px;
		height: 6px;
	}

	:global(:root[data-theme='dark']) .banner {
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
	}

	.content {
		flex: 1;
		min-width: 0;
		width: 100%;
		max-width: 1240px;
		padding: calc(var(--topbar-h) + var(--banner-h, 0px) + var(--space-2)) var(--gutter)
			var(--space-16);
		display: flex;
		flex-direction: column;
		gap: var(--space-10);
	}

	.content > :global(*) {
		min-width: 0;
	}

	/* ---------------------------------------------------------------------------- the foot */
	.foot {
		border-top: var(--rule);
		padding: var(--space-6) var(--gutter);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 1240px;
		width: 100%;
	}

	.foot-in {
		display: flex;
		align-items: center;
		gap: var(--space-5);
		flex-wrap: wrap;
	}

	.brand.small .mark {
		width: 26px;
		height: 26px;
		border-radius: 9px;
		font-size: 15px;
		background: var(--fg);
		color: var(--bg);
	}

	.brand.small {
		padding: 0;
	}

	.brand.small .name {
		font-size: var(--fs-body);
	}

	.foot-line {
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}

	.foot-links {
		display: flex;
		gap: var(--space-5);
		margin-left: auto;
		font-size: var(--fs-data);
		font-weight: 500;
		color: var(--fg-muted);
	}

	.mainnet {
		border: var(--rule);
		color: var(--fg-muted);
	}

	.copy {
		font-size: var(--fs-micro);
		color: var(--fg-muted);
	}

	/* -------------------------------------------------------------------- phone navigation */
	.tabbar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		z-index: 40;
		display: flex;
		background: var(--ink-panel);
		color: var(--ink-fg-muted);
		padding: 6px 4px calc(6px + env(safe-area-inset-bottom));
	}

	.tabbar a {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 3px;
		padding: 6px 2px;
		border-radius: var(--radius-control);
		font-size: 10.5px;
		font-weight: 600;
		color: inherit;
	}

	.tabbar a.current {
		color: var(--accent);
	}

	@media (min-width: 900px) {
		.shell {
			grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
		}

		.tabbar {
			display: none;
		}

		.rail-in {
			position: sticky;
			top: 0;
			height: 100vh;
		}
	}

	/* Under 900 px the dark column becomes a dark strip: brand and chain height only, with
	   navigation moved to the thumb at the bottom of the screen. */
	@media (max-width: 899px) {
		.rail-in {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
			gap: var(--space-3);
			padding: var(--space-3) var(--gutter);
		}

		.nav,
		.note {
			display: none;
		}

		.rail-foot {
			margin: 0;
		}

		.chain {
			flex-direction: row;
			align-items: center;
			gap: var(--space-3);
			padding: 6px var(--space-3);
			background: none;
			border: 0;
		}

		.chain-row,
		.chain-head {
			margin: 0;
		}

		.chain-row:not(:first-of-type) {
			display: none;
		}

		.chain-row {
			gap: 6px;
		}

		.chain-row span {
			display: none;
		}

		.status-slot {
			display: none;
		}

		.content {
			padding-bottom: 96px;
		}

		.foot {
			padding-bottom: 96px;
		}

		.foot-links {
			margin-left: 0;
		}
	}
</style>
