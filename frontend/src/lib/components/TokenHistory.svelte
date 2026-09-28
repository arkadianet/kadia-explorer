<script lang="ts">
	import type { TxSummaryDto } from '$lib/api/types';
	import { createPager, type Pager } from '$lib/pager/pager.svelte';
	import { tokenHistory, type TokenHistoryPage } from '$lib/token/history';
	import InfiniteList from './InfiniteList.svelte';
	import Hash from './Hash.svelte';
	import Amount from './Amount.svelte';
	import Age from './Age.svelte';
	import Skeleton from './Skeleton.svelte';
	let { id }: { id: string } = $props();
	let pager = $state.raw<Pager<TxSummaryDto> | null>(null);
	let metadata = $state<TokenHistoryPage | null>(null);
	$effect(() => {
		const tokenId = id;
		metadata = null;
		const current = createPager<TxSummaryDto>(async (cursor, snapshot) => {
			const result = await tokenHistory(tokenId, cursor, snapshot);
			if (pager === current) metadata = result;
			return result;
		});
		pager = current;
		void current.loadMore();
		return () => current.reset();
	});
</script>

<section class="token-history" aria-label="Token transactions">
	<header>
		<h3>Where this token appears</h3>
		<p>
			Transactions containing this token in a resolved input or an output, newest first. This
			includes minting, burning, change and contract movements; it does not identify payments or a
			transfer amount.
		</p>
	</header>
	{#if metadata && !pager?.restartRequired}
		<p class="coverage">
			{#if metadata.anchor}
				Snapshot at indexed block <a href={`/blocks/${metadata.anchor.height}`}
					>{metadata.anchor.height.toLocaleString('en-US')}</a
				>.
			{/if}
			{#if metadata.history_context.partial_from !== null}
				Partial index starting at block {metadata.history_context.partial_from.toLocaleString(
					'en-US'
				)}. Earlier transactions and transactions whose only token involvement is in unresolved
				inputs may be absent.
			{:else}
				Covers token touches in the retained canonical index. Each transaction appears once, even if
				several boxes contain this token.
			{/if}
		</p>
	{/if}
	{#if pager}
		<InfiniteList
			table
			dense
			columns={6}
			{pager}
			empty="No transaction touching this token was found in the indexed range."
		>
			{#snippet head()}
				<tr
					><th>Transaction</th><th>Block</th><th>Age</th><th class="num">Inputs</th><th class="num"
						>Outputs</th
					><th class="num">Transaction fee</th></tr
				>
			{/snippet}
			{#snippet children(tx: TxSummaryDto)}
				<tr>
					<td><Hash value={tx.id} href={`/tx/${tx.id}`} copy={false} /></td>
					<td class="mono"><a href={`/blocks/${tx.height}`}>{tx.height}</a></td>
					<td><Age ms={tx.timestamp} /></td>
					<td class="num mono">{tx.input_count}</td>
					<td class="num mono">{tx.output_count}</td>
					<td class="num"><Amount nano={tx.fee} maxFrac={3} /></td>
				</tr>
			{/snippet}
		</InfiniteList>
	{:else}<Skeleton />{/if}
</section>

<style>
	.token-history {
		min-width: 0;
	}
	header {
		display: grid;
		grid-template-columns: minmax(160px, 0.7fr) minmax(0, 1.3fr);
		gap: 20px;
		padding: 8px 0 20px;
	}
	h3 {
		font-family: var(--font-display, var(--font-sans));
		font-size: 24px;
		font-weight: var(--weight-display, 600);
	}
	p {
		font-size: 12px;
		line-height: 1.7;
		color: var(--fg-muted);
	}
	.coverage {
		margin-bottom: 20px;
		padding: 12px 16px;
		background: var(--surface-hover);
		border-left: 2px solid var(--accent-ink);
	}
	@media (max-width: 600px) {
		header {
			display: block;
		}
		header p {
			margin-top: 10px;
		}
	}
</style>
