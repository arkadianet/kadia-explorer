<script lang="ts">
	import '$lib/styles/tokens.css';
	import '$lib/styles/base.css';
	import { page } from '$app/state';
	import { theme } from '$lib/theme/theme.svelte';
	import { status } from '$lib/status/status.svelte';
	import StatusBadge from '$lib/components/StatusBadge.svelte';
	import SearchBox from '$lib/components/SearchBox.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	// The lag/stalled banner lives here rather than on the home page so every route surfaces a
	// degraded index — the status store is the single source, polled every 5 s by `status.start()`.
	const stalledInfo = $derived(status.current?.stalled ?? null);
	const showLagBanner = $derived.by(() => {
		const s = status.current;
		return s !== null && (s.lag_blocks > 100 || s.halted !== null);
	});

	const navLinks = [
		{ href: '/', label: 'Overview' },
		{ href: '/blocks', label: 'Blocks' },
		{ href: '/txs', label: 'Transactions' },
		{ href: '/richlist', label: 'Rich list' },
		{ href: '/rent', label: 'Rent' },
		{ href: '/status', label: 'Status' }
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

	onMount(() => {
		theme.init();
		status.start();
		return () => status.stop();
	});
</script>

<div class="shell">
	<header class="topbar">
		<a class="brand" href="/">
			kadia<span class="brand-dot" aria-hidden="true"></span><span class="brand-sub">explorer</span>
		</a>
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
			{#if theme.current === 'dark'}
				<svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
					<path fill="currentColor" d="M13.3 9.9A5.6 5.6 0 0 1 6.1 2.7 5.7 5.7 0 1 0 13.3 9.9Z" />
				</svg>
			{:else}
				<svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
					<circle cx="8" cy="8" r="3.1" fill="currentColor" />
					<g stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
						<path
							d="M8 1v1.8M8 13.2V15M1 8h1.8M13.2 8H15M3.1 3.1l1.3 1.3M11.6 11.6l1.3 1.3M12.9 3.1l-1.3 1.3M4.4 11.6l-1.3 1.3"
						/>
					</g>
				</svg>
			{/if}
		</button>
	</header>

	{#if stalledInfo}
		<div class="banner danger" role="alert">
			Indexer is waiting on block {stalledInfo.height} from the node for {stalledInfo.since_secs}
			s.
		</div>
	{:else if showLagBanner && status.current}
		<div class="banner warn" role="status">
			Index is {status.current.lag_blocks} blocks behind the node.
		</div>
	{/if}

	<nav class="rail" aria-label="Primary">
		<ul>
			{#each navLinks as link (link.href)}
				<li>
					<a
						href={link.href}
						class:current={isCurrent(link.href)}
						aria-current={isActive(link.href) ? 'page' : undefined}
					>
						{link.label}
					</a>
				</li>
			{/each}
		</ul>
	</nav>

	<main class="content">
		{@render children()}
	</main>
</div>

<style>
	.shell {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		grid-template-rows: auto auto 1fr auto;
		grid-template-areas:
			'topbar'
			'banner'
			'content'
			'rail';
		min-height: 100vh;
	}

	.topbar {
		grid-area: topbar;
		display: flex;
		align-items: center;
		gap: var(--space-4);
		height: 52px;
		padding: 0 var(--space-4);
		border-bottom: var(--rule);
	}

	.brand {
		display: inline-flex;
		align-items: baseline;
		gap: var(--space-2);
		font-weight: 600;
		font-size: var(--fs-title);
		letter-spacing: -0.01em;
		white-space: nowrap;
	}

	.brand:hover,
	.brand:focus-visible {
		color: inherit;
	}

	.brand-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--accent);
		align-self: center;
	}

	.brand-sub {
		font-weight: 400;
		font-size: var(--fs-body);
		color: var(--fg-muted);
	}

	.search-slot {
		flex: 1;
		min-width: 0;
	}

	/* Under 720 px there is no room for a usable search field beside the wordmark, so it takes
	   a line of its own rather than being squeezed to a stub. */
	@media (max-width: 719px) {
		.topbar {
			height: auto;
			flex-wrap: wrap;
			gap: var(--space-2) var(--space-3);
			padding: var(--space-2) var(--space-4);
		}

		.search-slot {
			order: 3;
			flex: 1 0 100%;
			max-width: none;
		}

		.status-slot {
			margin-left: auto;
		}
	}

	.status-slot {
		flex-shrink: 0;
	}

	.theme-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		flex-shrink: 0;
		background: transparent;
		border: var(--rule);
		border-radius: var(--radius);
		color: var(--fg-muted);
		cursor: pointer;
	}

	.theme-toggle:hover {
		color: var(--fg);
		background: var(--bg-hover);
	}

	/* A rule-bounded strip rather than a boxed alert: the index being behind is a condition of
	   the whole page, not a message sitting inside it. */
	.banner {
		grid-area: banner;
		padding: var(--space-2) var(--space-4);
		font-size: var(--fs-data);
		border-top: 1px solid currentcolor;
		border-bottom: 1px solid currentcolor;
	}

	.banner.warn {
		color: var(--warn-ink);
	}

	.banner.danger {
		color: var(--danger-ink);
	}

	.rail {
		grid-area: rail;
		align-self: start;
		min-width: 0;
		border-top: var(--rule);
		background: var(--bg);
	}

	.rail ul {
		display: flex;
		overflow-x: auto;
	}

	.rail a {
		display: block;
		padding: var(--space-3) var(--space-4);
		color: var(--fg-muted);
		white-space: nowrap;
		border-top: 3px solid transparent;
	}

	.rail a:hover {
		color: var(--fg);
	}

	.rail a.current {
		color: var(--fg);
		border-top-color: var(--accent);
	}

	.content {
		grid-area: content;
		max-width: 1280px;
		margin: 0 auto;
		width: 100%;
		/* Grid children default to min-width:auto; without this a wide table or the block strip
		   would push the whole shell into a horizontal scroll. */
		min-width: 0;
		padding: var(--space-6) var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-8);
	}

	.content > :global(*) {
		min-width: 0;
	}

	@media (min-width: 1024px) {
		.shell {
			grid-template-columns: 200px minmax(0, 1fr);
			grid-template-rows: auto auto 1fr;
			grid-template-areas:
				'topbar topbar'
				'banner banner'
				'rail content';
		}

		.rail {
			border-top: none;
			border-right: var(--rule);
			padding-top: var(--space-4);
			min-height: 100%;
		}

		.rail ul {
			display: block;
			overflow-x: visible;
		}

		.rail a {
			padding: var(--space-2) var(--space-4);
			border-top: 0;
			border-left: 3px solid transparent;
		}

		.rail a.current {
			border-left-color: var(--accent);
		}

		.content {
			padding: var(--space-6);
		}
	}
</style>
