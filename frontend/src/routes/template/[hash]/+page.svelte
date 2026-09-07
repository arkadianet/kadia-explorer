<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
	import InfiniteList from '$lib/components/InfiniteList.svelte';
	import BoxCard from '$lib/components/BoxCard.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Skeleton from '$lib/components/Skeleton.svelte';
	import { api } from '$lib/api/endpoints';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { status } from '$lib/status/status.svelte';
	import { truncateMiddle } from '$lib/format/hash';
	import type { BoxDto } from '$lib/api/types';
	import type { PageData } from './$types';

	const PAGE_SIZE = 50;

	let { data }: { data: PageData } = $props();

	const hash = $derived(data.hash);
	const template = $derived(data.template);
	const tip = $derived(status.current?.indexed ?? null);

	/** Unspent first: a template is usually looked up to see what is currently locked by it. */
	let unspentOnly = $state(true);
	let pager = $state<Pager<BoxDto> | null>(null);

	// Navigating from one template to another reuses this component, so the pager and the
	// filter have to be dropped explicitly or the new hash shows the old one's boxes.
	// Plain `let` (not `$state`) so reading it here does not make this effect depend on it.
	let loadedFor: string | null = null;

	$effect(() => {
		if (hash === loadedFor) return;
		loadedFor = hash;
		unspentOnly = true;
		pager = null;
	});

	$effect(() => {
		if (template === null || pager) return;
		startBoxes();
	});

	function startBoxes() {
		const p = createPager<BoxDto>((c) => api.templateBoxes(hash, unspentOnly, c, PAGE_SIZE));
		pager = p;
		void p.loadMore();
	}

	function selectUnspent(next: boolean) {
		if (next === unspentOnly) return;
		unspentOnly = next;
		// A different filter is a different query: start a fresh pager rather than appending
		// the other set's rows to the ones already on screen.
		startBoxes();
	}

	const boxCount = $derived(template?.box_count ?? 0);
	const unspentCount = $derived(template?.unspent_count ?? 0);
</script>

<svelte:head>
	<title
		>{template ? `Script template ${truncateMiddle(hash)}` : 'Script template not found'} — Ergo Explorer</title
	>
</svelte:head>

{#if template === null}
	<div class="head">
		<PageHead title="Script template" id={hash} />
		<EmptyState
			message="Template not found — no box in the index is locked by a script with this template hash. Check the hash, or come back once the indexer has reached the block that created such a box."
		/>
	</div>
{:else}
	<div class="head">
		<PageHead title="Script template" id={template.hash} />

		<p class="lede">
			The hash of an ergo tree with its constants stripped out: every box below is locked by the
			same script shape, whatever values were baked into it.
		</p>

		<Facts>
			<Fact label="Boxes">
				<span class="mono">{boxCount.toLocaleString('en-US')}</span>
			</Fact>
			<Fact label="Unspent">
				<span class="mono">{unspentCount.toLocaleString('en-US')}</span>
			</Fact>
			<Fact label="First seen">
				<a class="mono" href={`/blocks/${template.first_seen}`}>{template.first_seen}</a>
			</Fact>
			<Fact label="Example address">
				{#if template.example_address}
					<Hash
						value={template.example_address}
						href={`/address/${template.example_address}`}
						copy={false}
						head={12}
					/>
				{:else}
					<span class="muted" title="No box with this template has a P2PK/P2S address">—</span>
				{/if}
			</Fact>
		</Facts>
	</div>

	<Panel title="Boxes">
		{#snippet actions()}
			<div class="seg" role="group" aria-label="Filter boxes">
				<button
					type="button"
					class="seg-btn"
					aria-pressed={unspentOnly}
					onclick={() => selectUnspent(true)}>Unspent</button
				>
				<button
					type="button"
					class="seg-btn"
					aria-pressed={!unspentOnly}
					onclick={() => selectUnspent(false)}>All</button
				>
			</div>
			<span class="count">
				{unspentOnly
					? `${unspentCount.toLocaleString('en-US')} unspent`
					: `${boxCount.toLocaleString('en-US')} ever`}
			</span>
		{/snippet}

		{#if pager}
			<div class="boxes">
				<InfiniteList
					{pager}
					empty={unspentOnly
						? 'No unspent box is locked by this template.'
						: 'No box in the indexed range is locked by this template.'}
				>
					{#snippet children(box: BoxDto)}
						<BoxCard {box} {tip} role="output" />
					{/snippet}
				</InfiniteList>
			</div>
		{:else}
			<Skeleton />
		{/if}
	</Panel>
{/if}

<style>
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	/* A template hash is the one entity here whose meaning is not self-evident from its name,
	   so the page says what it is before it starts listing boxes. */
	.lede {
		color: var(--fg-muted);
		font-size: var(--fs-data);
		max-width: 72ch;
	}

	.muted {
		color: var(--fg-muted);
	}

	.count {
		font-size: var(--fs-micro);
		white-space: nowrap;
	}

	.boxes :global(.list) {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
</style>
