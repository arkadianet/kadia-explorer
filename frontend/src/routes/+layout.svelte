<script lang="ts">
	import '$lib/styles/tokens.css';
	import '$lib/styles/base.css';
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
		{ href: '/blocks', label: 'Blocks' },
		{ href: '/txs', label: 'Transactions' },
		{ href: '/richlist', label: 'Rich list' },
		{ href: '/rent', label: 'Rent' },
		{ href: '/status', label: 'Status' }
	];

	onMount(() => {
		theme.init();
		status.start();
		return () => status.stop();
	});
</script>

<div class="shell">
	<header class="topbar">
		<a class="brand" href="/">Ergo Explorer</a>
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
			{theme.current === 'dark' ? '🌙' : '☀️'}
		</button>
	</header>

	<nav class="rail" aria-label="Primary">
		<ul>
			{#each navLinks as link (link.href)}
				<li><a href={link.href}>{link.label}</a></li>
			{/each}
		</ul>
	</nav>

	<main class="content">
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
		{@render children()}
	</main>
</div>

<style>
	.shell {
		display: grid;
		grid-template-columns: 1fr;
		grid-template-rows: auto 1fr auto;
		grid-template-areas:
			'topbar'
			'content'
			'rail';
		min-height: 100vh;
	}

	.topbar {
		grid-area: topbar;
		display: flex;
		align-items: center;
		gap: var(--space-4);
		padding: var(--space-3) var(--space-4);
		background: var(--bg-elev);
		border-bottom: 1px solid var(--border);
	}

	.brand {
		font-weight: 600;
		color: var(--fg);
		text-decoration: none;
		white-space: nowrap;
	}

	.search-slot {
		flex: 1;
	}

	.theme-toggle {
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: var(--space-1) var(--space-2);
		cursor: pointer;
	}

	.theme-toggle:hover {
		background: var(--bg-hover);
	}

	.rail {
		grid-area: rail;
		align-self: start;
		background: var(--bg-elev);
		border-top: 1px solid var(--border);
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
	}

	.rail a:hover {
		color: var(--fg);
		background: var(--bg-hover);
		text-decoration: none;
	}

	.banner {
		margin-bottom: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius);
		border: 1px solid var(--border);
		background: var(--bg-elev);
	}

	.banner.warn {
		border-color: var(--warn);
		color: var(--warn);
	}

	.banner.danger {
		border-color: var(--danger);
		color: var(--danger);
	}

	.content {
		grid-area: content;
		max-width: 1280px;
		margin: 0 auto;
		width: 100%;
		padding: var(--space-4);
	}

	@media (min-width: 1024px) {
		.shell {
			grid-template-columns: 220px 1fr;
			grid-template-rows: auto 1fr;
			grid-template-areas:
				'topbar topbar'
				'rail content';
		}

		.rail {
			grid-area: rail;
			border-top: none;
			border-right: 1px solid var(--border);
		}

		.rail ul {
			display: block;
			overflow-x: visible;
		}

		.rail a {
			padding: var(--space-2) var(--space-4);
		}
	}
</style>
