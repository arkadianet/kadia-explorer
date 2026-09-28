<script lang="ts">
	import Panel from '$lib/components/Panel.svelte';
	import PageHead from '$lib/components/PageHead.svelte';
	import Facts from '$lib/components/Facts.svelte';
	import Fact from '$lib/components/Fact.svelte';
	import BlockTransactions from '$lib/components/BlockTransactions.svelte';
	import Hash from '$lib/components/Hash.svelte';
	import MinerChip from '$lib/components/MinerChip.svelte';
	import Amount from '$lib/components/Amount.svelte';
	import Age from '$lib/components/Age.svelte';
	import ErrorState from '$lib/components/ErrorState.svelte';
	import { formatNano } from '$lib/format/amount';
	import { formatKb } from '$lib/format/size';
	import { absTime } from '$lib/format/time';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	const block = $derived(data.block);
</script>

<svelte:head>
	<title>Block {block.height} — Ergo Explorer</title>
</svelte:head>

<div class="head">
	<PageHead title={`Block ${block.height}`} id={block.id} />

	<Facts>
		<Fact label="Height"><span class="mono">{block.height}</span></Fact>
		<Fact label="Parent">
			<Hash value={block.parent_id} href={`/blocks/${block.parent_id}`} copy={false} />
		</Fact>
		<Fact label="Mined"><span>{absTime(block.timestamp)}, <Age ms={block.timestamp} /></span></Fact>
		<Fact label="Difficulty"><span class="mono">{formatNano(block.difficulty)}</span></Fact>
		<Fact label="Size"><span class="mono">{formatKb(block.size)}</span></Fact>
		<Fact label="Version"><span class="mono">{block.version}</span></Fact>
		<Fact label="Txs"><span class="mono">{block.tx_count}</span></Fact>
		<Fact label="Fees"><Amount nano={block.fees} maxFrac={3} /></Fact>
		<Fact label="Miner"><MinerChip minerPk={block.miner_pk} /></Fact>
	</Facts>

	<nav class="pager" aria-label="Adjacent blocks">
		{#if block.height > 1}
			<a href={`/blocks/${block.height - 1}`}>&larr; Prev</a>
		{:else}
			<span></span>
		{/if}
		<a href={`/blocks/${block.height + 1}`}>Next &rarr;</a>
	</nav>
</div>

<Panel title="Emission reward explained">
	{#if data.rewards.error}
		<ErrorState error={data.rewards.error} />
	{:else if data.rewards.data?.basis === 'observed_eip27_reward_box'}
		{@const reward = data.rewards.data}
		<div class="reward-equation">
			<div>
				<span>Gross reward box</span><strong><Amount nano={reward.gross_reward!} /></strong>
			</div>
			<span class="operator" aria-label="minus">−</span>
			<div>
				<span>Re-emission obligation</span><strong
					><Amount nano={reward.reemission_obligation!} /></strong
				>
			</div>
			<span class="operator" aria-label="equals">=</span>
			<div class="net">
				<span>Miner subsidy</span><strong><Amount nano={reward.miner_subsidy!} /></strong>
			</div>
		</div>
		<p class="reward-note">{reward.note}</p>
		<p class="reward-source">
			Observed reward box: <Hash
				value={reward.reward_box_id!}
				href={`/box/${reward.reward_box_id}`}
			/> ·
			<a
				href="https://github.com/ergoplatform/eips/blob/master/eip-0027.md"
				target="_blank"
				rel="noreferrer">EIP-27 definition</a
			>
		</p>
	{:else}
		<p class="reward-note">
			{data.rewards.data?.note ?? 'Reward evidence is unavailable for this block.'}
		</p>
	{/if}
</Panel>

<Panel title="Transactions">
	{#key block.id}<BlockTransactions blockId={block.id} />{/key}
</Panel>

<style>
	.reward-equation {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-5);
		padding-block: var(--space-3);
	}
	.reward-equation div {
		display: grid;
		gap: var(--space-2);
	}
	.reward-equation div > span {
		color: var(--fg-muted);
		font-size: var(--fs-micro);
	}
	.reward-equation strong {
		font-size: var(--fs-key);
		font-weight: 600;
	}
	.reward-equation .net {
		color: var(--accent-ink);
	}
	.operator {
		color: var(--fg-muted);
		font-size: var(--fs-key);
	}
	.reward-note,
	.reward-source {
		color: var(--fg-muted);
		font-size: var(--fs-data);
		line-height: 1.6;
		margin-top: var(--space-3);
		overflow-wrap: anywhere;
	}
	:global(html[data-appearance='prism']) .reward-equation {
		border-left: 3px solid var(--accent);
		padding-left: var(--space-4);
	}
	:global(html[data-appearance='atelier']) .reward-equation {
		justify-content: space-between;
		border-block: 1px solid var(--border);
	}
	:global(html[data-appearance='aurora']) .reward-equation div {
		background: var(--surface-solid);
		border: 1px solid var(--border);
		border-radius: 18px;
		padding: var(--space-4);
	}
	@media (max-width: 540px) {
		.reward-equation {
			flex-direction: column;
			align-items: stretch;
			gap: var(--space-2);
		}
		.operator {
			text-align: center;
		}
	}
	.head {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.pager {
		display: flex;
		justify-content: space-between;
		font-size: var(--fs-data);
		color: var(--fg-muted);
	}
	.pager a:hover,
	.pager a:focus-visible {
		color: var(--accent-ink);
	}
</style>
