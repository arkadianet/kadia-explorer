<script lang="ts">
	import { untrack } from 'svelte';
	import { api } from '$lib/api/endpoints';
	import {
		createSpectrumWorkflow,
		type SpectrumOrder,
		type WorkflowAsset,
		type WorkflowState
	} from '$lib/tx/spectrum';
	import { formatErg } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Icon from './Icon.svelte';

	let { order }: { order: SpectrumOrder } = $props();
	let controller: ReturnType<typeof createSpectrumWorkflow> | undefined;
	let state = $state<WorkflowState>({
		loading: false,
		checked: false,
		error: null,
		box: null,
		tx: null,
		outcome: null
	});
	const orderKey = $derived(
		JSON.stringify([
			order.box.id,
			order.box.ergo_tree,
			order.box.value,
			order.box.tokens,
			order.box.registers,
			order.box.creation_height,
			order.box.spent_by,
			order.box.spent_height
		])
	);
	$effect(() => {
		void orderKey;
		const current = untrack(() => order);
		state = { loading: false, checked: false, error: null, box: null, tx: null, outcome: null };
		const workflow = createSpectrumWorkflow({
			order: current,
			getBox: api.box,
			getTx: api.tx,
			onChange: (next) => {
				state = next;
			}
		});
		controller = workflow;
		return () => workflow.stop();
	});
	const short = (id: string) => truncateMiddle(id, 10, 7);
	const matched = $derived(state.outcome?.kind === 'matched' ? state.outcome : null);
	const isSwap = $derived(order.kind === 'swap_sell' || order.kind === 'swap_buy');
	const requested = $derived.by((): WorkflowAsset[] => {
		switch (order.kind) {
			case 'swap_sell':
				return [{ id: 'erg', amount: order.baseNano.toString() }];
			case 'swap_buy':
				return [{ id: order.baseId, amount: order.baseAmount.toString() }];
			case 'deposit':
				return [
					{ id: 'erg', amount: order.baseNano.toString() },
					{ id: order.tokenId, amount: order.tokenAmount.toString() }
				];
			case 'redeem':
				return [{ id: order.lpId, amount: order.lpAmount.toString() }];
		}
	});
	const minimum = $derived(
		order.kind === 'swap_sell'
			? { id: order.quoteId, amount: order.minimumQuote.toString() }
			: order.kind === 'swap_buy'
				? { id: 'erg', amount: order.minimumNano.toString() }
				: null
	);
	const intro = $derived(
		order.kind === 'swap_sell'
			? 'An ERG → token order, from its request to the observed spend.'
			: order.kind === 'swap_buy'
				? 'A token → ERG order, from its request to the observed spend.'
				: order.kind === 'deposit'
					? 'An ERG + token deposit, from its request to the observed LP output.'
					: 'An LP redemption, from its request to the observed ERG and token outputs.'
	);
	const stage = $derived(
		state.error
			? 'Inspection unavailable'
			: matched
				? isSwap
					? 'Matching swap observed'
					: order.kind === 'deposit'
						? 'Matching deposit observed'
						: 'Matching redemption observed'
				: state.tx
					? 'Spent · outcome unrecognized'
					: state.checked
						? 'No indexed spend observed'
						: 'Ready to trace'
	);
	function inspect() {
		untrack(() => {
			void controller?.inspect();
		});
	}
</script>

{#snippet assetDetail(asset: WorkflowAsset)}
	{#if asset.id === 'erg'}
		<span title={asset.amount + ' nanoERG'}>{formatErg(asset.amount)} ERG</span>
	{:else}
		<span>{BigInt(asset.amount).toLocaleString('en-US')} raw units</span><br /><a
			href={'/token/' + asset.id}
			title={asset.id}>{short(asset.id)}</a
		>
	{/if}
{/snippet}

<section
	class="application-workflow"
	aria-label={isSwap ? 'Spectrum swap workflow' : 'Spectrum liquidity workflow'}
	aria-busy={state.loading}
>
	<header>
		<div class="workflow-brand">
			<Icon name="layers" size={18} /><span>APPLICATION WORKFLOW / SPECTRUM V3</span>
		</div>
		<div class="workflow-title">
			<div>
				<h2>
					{isSwap
						? 'Follow the swap.'
						: order.kind === 'deposit'
							? 'Follow the deposit.'
							: 'Follow the redemption.'}
				</h2>
				<p>{intro}</p>
			</div>
			<span class:matched={!!matched} class="stage" role="status"
				>{state.loading ? 'Inspecting indexed evidence…' : stage}</span
			>
		</div>
	</header>
	<div class="workflow-body">
		<div class="request-brief">
			<p class="step-label">01 / THE REQUEST</p>
			{#each requested as asset (asset.id)}
				<p
					class="base-amount"
					title={asset.amount + (asset.id === 'erg' ? ' nanoERG' : ' raw units')}
				>
					{asset.id === 'erg'
						? formatErg(asset.amount)
						: BigInt(asset.amount).toLocaleString('en-US')}
					<span
						>{asset.id === 'erg'
							? 'ERG'
							: order.kind === 'redeem'
								? 'raw LP units'
								: 'raw token units'}</span
					>
				</p>
				{#if asset.id !== 'erg'}<a
						class="request-asset"
						href={'/token/' + asset.id}
						title={asset.id}>{short(asset.id)}</a
					>{/if}
			{/each}
			<p class="muted">
				Declared {isSwap
					? 'swap input'
					: order.kind === 'deposit'
						? 'deposit amounts'
						: 'LP input'}. The order box also carries funding for other outputs.
			</p>
			<dl>
				{#if minimum}
					<div>
						<dt>Minimum quote</dt>
						<dd>{@render assetDetail(minimum)}</dd>
					</div>
				{:else}
					<div>
						<dt>Return bound</dt>
						<dd>Calculated from the pool reserves when the recorded spend is inspected.</dd>
					</div>
				{/if}
				<div>
					<dt>Pool identity token</dt>
					<dd><a href={'/token/' + order.poolId} title={order.poolId}>{short(order.poolId)}</a></dd>
				</div>
				<div>
					<dt>Order box</dt>
					<dd><a href={'/box/' + order.box.id} title={order.box.id}>{short(order.box.id)}</a></dd>
				</div>
			</dl>
			<a class="creation-link" href={'/tx/' + order.box.tx_id}
				>Open order creation <span aria-hidden="true">↗</span></a
			>
		</div>
		<div class="execution">
			<p class="step-label">02 / THE OBSERVED OUTCOME</p>
			{#if state.error}
				<div class="workflow-notice" role="alert">
					<h3>Evidence could not be loaded</h3>
					<p>{state.error}</p>
					<p>No automatic retries run. Inspect again to recheck.</p>
				</div>
			{:else if matched && state.tx}
				{#each matched.received as asset (asset.id)}
					<div class="result-number">
						<span>Recipient output{order.kind === 'deposit' ? ' · LP tokens' : ''}</span>
						<strong title={asset.amount + (asset.id === 'erg' ? ' nanoERG' : ' raw units')}
							>{asset.id === 'erg'
								? formatErg(asset.amount)
								: BigInt(asset.amount).toLocaleString('en-US')}</strong
						>
						<span
							>{#if asset.id === 'erg'}ERG · {asset.amount} nanoERG{:else}raw token units · <a
									href={'/token/' + asset.id}>{short(asset.id)}</a
								>{/if}</span
						>
					</div>
				{/each}
				<p class="result-caption">
					{order.kind === 'swap_sell'
						? 'This output can include returned execution-fee tokens.'
						: order.kind === 'deposit'
							? 'LP tokens represent the observed pool-share output; other returned assets are in the full box record.'
							: 'The ERG output includes returned order-box funding.'} It is not an address’s net gain.
				</p>
				<dl class="execution-facts">
					<div>
						<dt>
							{isSwap
								? 'Quote calculated by the contract'
								: order.kind === 'deposit'
									? 'Minimum LP return'
									: 'Minimum redemption return'}
						</dt>
						<dd>
							{#each matched.calculated as asset (asset.id)}<div>
									{@render assetDetail(asset)}
								</div>{/each}
						</dd>
					</div>
					<div>
						<dt>Spend included at</dt>
						<dd>
							<a href={'/blocks/' + state.tx.block_id}
								>Block {state.tx.height.toLocaleString('en-US')}</a
							>
						</dd>
					</div>
				</dl>
				<div class="evidence-links">
					<a href={'/tx/' + state.tx.id}>Spending transaction ↗</a><a
						href={'/box/' + matched.recipient.id}>Recipient output ↗</a
					><a href={'/box/' + matched.poolBefore.id}>Pool before ↗</a><a
						href={'/box/' + matched.poolAfter.id}>Pool after ↗</a
					>
				</div>
			{:else if state.outcome?.kind === 'unrecognized' && state.tx}
				<div class="workflow-notice">
					<h3>Spent, with an unrecognized outcome</h3>
					<p>{state.outcome.reason}</p>
					<p>A spend alone does not establish a fill or cancellation.</p>
					<a href={'/tx/' + state.tx.id}>Inspect the spending transaction ↗</a>
				</div>
			{:else if state.checked}
				<div class="workflow-empty">
					<Icon name="clock" size={28} />
					<h3>No indexed spend observed</h3>
					<p>
						The current box record has no spending transaction. This does not check the mempool or
						promise execution.
					</p>
				</div>
			{:else}
				<div class="workflow-empty">
					<Icon name="search" size={28} />
					<h3>Inspect the next step.</h3>
					<p>
						Recheck this order box and, if spent, load its spending transaction. Requests run only
						when you choose.
					</p>
				</div>
			{/if}
			<div class="workflow-action">
				<button type="button" onclick={inspect} disabled={state.loading}
					>{state.loading
						? 'Inspecting…'
						: state.checked || state.error
							? 'Refresh workflow'
							: 'Inspect workflow'}<span aria-hidden="true"> ↗</span></button
				>
				{#if state.tx}<span class="muted"
						>Transaction details checked at {state.tx.indexed_height == null
							? 'an unavailable snapshot height'
							: 'indexed block ' + state.tx.indexed_height.toLocaleString('en-US')}.</span
					>{/if}
			</div>
		</div>
	</div>
	<details class="workflow-support">
		<summary>What this interpretation supports</summary>
		<p>
			Decoder v2 recognizes pinned Spectrum v3 ERG → token and token → ERG swaps, liquidity deposits
			and redemptions with P2PK recipients. It checks the exact published expressions, pool
			transitions, declared amounts, fee limits and recipient outputs. Support is limited to the
			documented contract forms and observed layouts. Token names do not establish identity.
		</p>
		<p>
			The order-box and transaction reads are separate observations. Changed references clear the
			result. This is an interpretation of indexed data, not a replay of consensus or proof of which
			signature branch was used. Other versions, refunds and bridges remain unrecognized.
		</p>
		<a href={order.source} target="_blank" rel="noopener noreferrer"
			>Read the pinned contract source ↗</a
		>
	</details>
</section>

<style>
	.application-workflow {
		margin-block: 28px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		overflow: hidden;
		background: var(--surface-solid);
		color: var(--fg);
		min-width: 0;
	}
	header {
		padding: 24px;
		border-bottom: 1px solid var(--hairline);
	}
	.workflow-brand {
		display: flex;
		gap: 9px;
		align-items: center;
		color: var(--accent-ink);
		font: 10px var(--font-mono);
		letter-spacing: 0.1em;
		margin-bottom: 20px;
	}
	.workflow-title {
		display: flex;
		gap: 20px;
		align-items: start;
		justify-content: space-between;
	}
	h2 {
		font-size: clamp(24px, 3vw, 36px);
		letter-spacing: -0.04em;
		line-height: 1.1;
	}
	.workflow-title p {
		margin-top: 10px;
		font-size: 12px;
		color: var(--fg-muted);
		max-width: 48ch;
		line-height: 1.7;
	}
	.stage {
		border: 1px solid var(--hairline);
		border-radius: 99px;
		padding: 8px 12px;
		font-size: 11px;
		flex-shrink: 0;
		max-width: 100%;
	}
	.stage.matched {
		color: var(--accent-ink);
		background: var(--accent-wash);
	}
	.workflow-body {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
	}
	.request-brief,
	.execution {
		padding: 24px;
		min-width: 0;
	}
	.request-brief {
		border-right: 1px solid var(--hairline);
	}
	.step-label {
		font: 10px var(--font-mono);
		letter-spacing: 0.1em;
		color: var(--fg-muted);
		margin-bottom: 22px;
	}
	.base-amount {
		font-family: var(--font-number, var(--font-sans));
		font-size: clamp(28px, 4vw, 43px);
		font-weight: var(--weight-number, 600);
		letter-spacing: -0.04em;
		overflow-wrap: anywhere;
	}
	.base-amount span {
		font: 12px var(--font-sans);
		letter-spacing: 0;
		color: var(--fg-muted);
	}
	.request-asset {
		display: inline-block;
		font: 11px var(--font-mono);
		margin-bottom: 12px;
	}
	.result-number + .result-number {
		margin-top: 18px;
	}
	.execution-facts dd > div + div {
		margin-top: 10px;
	}
	.muted {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.65;
	}
	dl {
		margin-block: 20px;
		display: grid;
		gap: 15px;
	}
	dl > div {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.25fr);
		gap: 10px;
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
	}
	dd {
		margin: 0;
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	dd a {
		font-family: var(--font-mono);
		font-size: 10px;
	}
	a {
		color: var(--accent-ink);
		text-decoration: underline;
		text-underline-offset: 3px;
		overflow-wrap: anywhere;
	}
	.creation-link {
		font-size: 12px;
	}
	.workflow-empty {
		padding-block: 22px 32px;
		color: var(--fg-muted);
		max-width: 42ch;
	}
	.workflow-empty h3,
	.workflow-notice h3 {
		color: var(--fg);
		font-size: 17px;
		margin-block: 12px 8px;
	}
	.workflow-empty p,
	.workflow-notice p {
		font-size: 12px;
		line-height: 1.8;
	}
	.workflow-notice {
		padding-block: 4px 20px;
	}
	.workflow-notice a {
		display: inline-block;
		margin-top: 10px;
		font-size: 12px;
	}
	.result-number {
		display: grid;
		gap: 8px;
		font-size: 11px;
		color: var(--fg-muted);
	}
	.result-number strong {
		font: var(--weight-number, 600) clamp(25px, 4vw, 43px) var(--font-number, var(--font-sans));
		color: var(--fg);
		letter-spacing: -0.035em;
		overflow-wrap: anywhere;
	}
	.result-caption {
		font-size: 11px;
		line-height: 1.7;
		color: var(--fg-muted);
		margin-top: 12px;
		max-width: 52ch;
	}
	.execution-facts {
		border-top: 1px solid var(--hairline);
		padding-top: 16px;
	}
	.evidence-links {
		display: flex;
		flex-wrap: wrap;
		gap: 10px 18px;
		font-size: 11px;
		margin-block: 18px;
	}
	.workflow-action {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
		margin-top: 18px;
	}
	button {
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		padding: 11px 16px;
		font-size: 12px;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.6;
		cursor: wait;
	}
	.workflow-support {
		border-top: 1px solid var(--hairline);
		padding: 16px 24px;
		font-size: 11px;
		line-height: 1.8;
	}
	summary {
		cursor: pointer;
		color: var(--fg-muted);
	}
	.workflow-support p {
		max-width: 100ch;
		margin-block: 10px;
	}
	:global([data-appearance='prism']) .application-workflow {
		box-shadow: 0 22px 55px color-mix(in srgb, var(--accent-ink) 13%, transparent);
		border-radius: 22px;
		background: linear-gradient(140deg, var(--surface-solid), var(--accent-wash));
	}
	:global([data-appearance='prism']) .request-brief {
		margin: 18px;
		border: 1px solid var(--hairline);
		border-radius: 16px;
		background: var(--surface-solid);
		box-shadow: 0 10px 22px color-mix(in srgb, var(--fg) 7%, transparent);
	}
	:global([data-appearance='prism']) .execution {
		padding: 36px 32px 32px 12px;
	}
	:global([data-appearance='atelier']) .application-workflow {
		border: 0;
		border-top: 3px solid var(--fg);
		border-radius: 0;
		background: transparent;
	}
	:global([data-appearance='atelier']) header {
		padding: 24px 0;
	}
	:global([data-appearance='atelier']) .workflow-body {
		grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
	}
	:global([data-appearance='atelier']) .request-brief {
		padding-left: 0;
	}
	:global([data-appearance='atelier']) .request-brief dl > div {
		grid-template-columns: 1fr;
		gap: 5px;
	}
	:global([data-appearance='atelier']) .workflow-support {
		padding-inline: 0;
	}
	:global([data-appearance='aurora']) .application-workflow {
		border-radius: 30px;
		padding: 20px;
		background: var(--accent-wash);
	}
	:global([data-appearance='aurora']) header {
		text-align: center;
		border: 0;
	}
	:global([data-appearance='aurora']) .workflow-brand {
		justify-content: center;
	}
	:global([data-appearance='aurora']) .workflow-title {
		display: grid;
		justify-items: center;
		justify-content: center;
	}
	:global([data-appearance='aurora']) .workflow-title p {
		margin-inline: auto;
	}
	:global([data-appearance='aurora']) .workflow-body {
		gap: 18px;
	}
	:global([data-appearance='aurora']) .request-brief,
	:global([data-appearance='aurora']) .execution {
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: 22px;
	}
	:global([data-appearance='aurora']) .workflow-support {
		border: 0;
	}
	@media (max-width: 720px) {
		.workflow-title {
			flex-direction: column;
		}
		.workflow-body,
		:global([data-appearance='atelier']) .workflow-body {
			grid-template-columns: 1fr;
		}
		.request-brief {
			border-right: 0;
			border-bottom: 1px solid var(--hairline);
		}
		header,
		.request-brief,
		.execution,
		.workflow-support {
			padding: 18px;
		}
		:global([data-appearance='prism']) .execution {
			padding: 18px;
		}
		:global([data-appearance='aurora']) .application-workflow {
			padding: 10px;
			border-radius: 22px;
		}
	}
</style>
