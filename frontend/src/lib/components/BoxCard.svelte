<script lang="ts">
	import Hash from './Hash.svelte';
	import Amount from './Amount.svelte';
	import RentBadge from './RentBadge.svelte';
	import RegistersTable from './RegistersTable.svelte';
	import { hasDisplayableRegisters } from '$lib/registers/decode';
	import { formatNano } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import type { BoxDto } from '$lib/api/types';

	interface Props {
		box: BoxDto;
		tip: number | null;
		role: 'input' | 'output';
	}

	let { box, tip, role }: Props = $props();

	const hasRegisters = $derived(hasDisplayableRegisters(box.registers));
</script>

<article class="box">
	<header>
		<Amount nano={box.value} maxFrac={9} />
		{#if role === 'output'}
			<RentBadge rent={box.rent} {tip} />
		{/if}
	</header>

	<dl>
		<dt>Address</dt>
		<dd>
			{#if box.address}
				<Hash value={box.address} href={`/address/${box.address}`} copy={false} head={10} />
			{:else}
				<span class="muted" title="No P2PK/P2S address — showing the ergo tree hash"
					><Hash value={box.tree_hash} copy={false} head={10} /></span
				>
			{/if}
		</dd>

		<dt>Box</dt>
		<dd><Hash value={box.id} href={`/box/${box.id}`} copy={false} head={10} /></dd>

		{#if box.spent_by}
			<dt>Spent by</dt>
			<dd>
				<Hash value={box.spent_by} href={`/tx/${box.spent_by}`} copy={false} head={10} />
				{#if box.spent_height !== null}
					<span class="muted"
						>at <a href={`/blocks/${box.spent_height}`}>{box.spent_height}</a></span
					>
				{/if}
			</dd>
		{/if}
	</dl>

	{#if box.tokens.length > 0}
		<div class="section">
			<span class="section-title">Tokens</span>
			<ul class="tokens">
				{#each box.tokens as token, i (i)}
					<li>
						<span class="mono" title={token.id}>{truncateMiddle(token.id)}</span>
						<span class="mono qty">{formatNano(token.amount)}</span>
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if hasRegisters}
		<div class="section">
			<span class="section-title">Registers</span>
			<RegistersTable registers={box.registers} />
		</div>
	{/if}
</article>

<style>
	.box {
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-elev);
		padding: var(--space-2) var(--space-3);
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		flex-wrap: wrap;
		margin-bottom: var(--space-2);
	}
	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 2px var(--space-3);
		margin: 0;
	}
	dt {
		color: var(--fg-muted);
	}
	dd {
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.muted {
		color: var(--fg-muted);
	}
	.section {
		margin-top: var(--space-2);
		border-top: 1px solid var(--border);
		padding-top: var(--space-2);
	}
	.section-title {
		color: var(--fg-muted);
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.tokens {
		list-style: none;
		margin: var(--space-1) 0 0;
		padding: 0;
	}
	.tokens li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.qty {
		color: var(--fg-muted);
	}
</style>
