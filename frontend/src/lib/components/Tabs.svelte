<script lang="ts">
	interface Tab {
		id: string;
		label: string;
		count?: number;
	}

	interface Props {
		tabs: Tab[];
		active: string;
		onchange: (id: string) => void;
	}

	let { tabs, active, onchange }: Props = $props();

	let refs: Record<string, HTMLButtonElement | undefined> = $state({});

	function move(delta: number) {
		if (tabs.length === 0) return;
		const at = tabs.findIndex((t) => t.id === active);
		const next = tabs[(((at < 0 ? 0 : at) + delta + tabs.length) % tabs.length)!];
		onchange(next.id);
		refs[next.id]?.focus();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
			e.preventDefault();
			move(1);
		} else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
			e.preventDefault();
			move(-1);
		} else if (e.key === 'Home') {
			e.preventDefault();
			move(-tabs.findIndex((t) => t.id === active));
		} else if (e.key === 'End') {
			e.preventDefault();
			move(tabs.length - 1 - tabs.findIndex((t) => t.id === active));
		}
	}
</script>

<div class="tabs" role="tablist">
	{#each tabs as tab (tab.id)}
		<button
			bind:this={refs[tab.id]}
			type="button"
			role="tab"
			id={`tab-${tab.id}`}
			aria-selected={tab.id === active}
			tabindex={tab.id === active ? 0 : -1}
			class="tab"
			class:selected={tab.id === active}
			onclick={() => onchange(tab.id)}
			{onkeydown}
		>
			{tab.label}{#if tab.count !== undefined}<span class="count">{tab.count}</span>{/if}
		</button>
	{/each}
</div>

<style>
	.tabs {
		display: flex;
		gap: var(--space-1);
		border-bottom: 1px solid var(--border);
	}
	.tab {
		background: none;
		border: 0;
		border-bottom: 2px solid transparent;
		padding: var(--space-2) var(--space-3);
		color: var(--fg-muted);
		cursor: pointer;
	}
	.tab:hover {
		color: var(--fg);
		background: var(--bg-hover);
	}
	.selected {
		color: var(--fg);
		border-bottom-color: var(--accent);
	}
	.count {
		margin-left: var(--space-1);
		color: var(--fg-muted);
		font-size: 11px;
	}
</style>
