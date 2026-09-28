<script lang="ts">
	import type { TokenHolderContext, TokenHolderDto } from '$lib/api/types';
	import { exactShare, holderConcentration } from '$lib/token/concentration';
	import { formatTokenAmount } from '$lib/format/amount';
	let {
		rows,
		context,
		decimals,
		loading,
		unavailable = false
	}: {
		rows: TokenHolderDto[];
		context: TokenHolderContext | null;
		decimals: number | null;
		loading: boolean;
		unavailable?: boolean;
	} = $props();
	const summary = $derived(unavailable ? null : holderConcentration(rows, context));
</script>

<section class="concentration" aria-labelledby="concentration-title">
	<div class="intro">
		<h3 id="concentration-title">Holder concentration</h3>
		<p>
			Balances are grouped by locking script, not by person. One script may hold funds for many
			people; one person may use many scripts.
		</p>
	</div>
	{#if loading && rows.length === 0}
		<p class="notice" role="status">Loading holder snapshot…</p>
	{:else if !summary}
		<p class="notice">
			Concentration is unavailable for this snapshot. A matching supply denominator is required; the
			holder rows remain available below.
		</p>
	{:else if summary.supply === 0n}
		<p class="notice">Indexed supply is zero. Percentage shares are undefined.</p>
	{:else if summary.loaded === 0}
		<p class="notice">
			No holder balances are available in this snapshot. This does not establish who owns the token.
		</p>
	{:else}
		<dl class="metrics">
			<div>
				<dt>Largest script balance</dt>
				<dd>{exactShare(summary.largest, summary.supply)}</dd>
			</div>
			<div>
				<dt>
					{summary.topCount > 1
						? `Top ${summary.topCount} loaded scripts`
						: 'Supply outside largest script'}
				</dt>
				<dd>
					{exactShare(
						summary.topCount > 1 ? summary.top : summary.supply - summary.largest,
						summary.supply
					)}
				</dd>
			</div>
			<div>
				<dt>Holder scripts loaded</dt>
				<dd class="loaded-count">
					{summary.loaded.toLocaleString('en-US')}
					<span>of {summary.total.toLocaleString('en-US')}</span>
				</dd>
			</div>
		</dl>
		<div class="coverage">
			<div class="bar" aria-hidden="true"><span style:width={`${summary.topWidth}%`}></span></div>
			<p>
				Top {summary.topCount} share of
				<strong>{formatTokenAmount(summary.supply.toString(), decimals)}</strong> indexed units after
				decimal scaling.
			</p>
		</div>
		<p class="definition">
			Supply means indexed minted amount minus indexed burns. {summary.loaded.toLocaleString(
				'en-US'
			)} of {summary.total.toLocaleString('en-US')} holder scripts are loaded, largest first. Percentages
			are truncated to two decimal places. This is not a full ownership distribution or a measure of decentralization.
		</p>
	{/if}
</section>

<style>
	.concentration {
		padding: 8px 0 28px;
		min-width: 0;
		border-bottom: var(--rule);
		margin-bottom: 20px;
	}
	.intro {
		display: grid;
		grid-template-columns: minmax(160px, 0.7fr) minmax(0, 1.3fr);
		gap: 20px;
		align-items: start;
	}
	h3 {
		font-family: var(--font-display, var(--font-sans));
		font-size: 24px;
		font-weight: var(--weight-display, 600);
	}
	p {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.65;
	}
	.metrics {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 20px;
		margin: 24px 0;
	}
	dt {
		color: var(--fg-muted);
		font-size: 12px;
	}
	dd {
		margin: 6px 0 0;
		color: var(--fg);
		font-size: clamp(24px, 3vw, 36px);
		font-family: var(--font-number, var(--font-mono));
		letter-spacing: -0.04em;
	}
	.loaded-count span {
		font-size: 13px;
		font-family: var(--font-sans);
		letter-spacing: 0;
		color: var(--fg-muted);
	}
	.bar {
		height: 6px;
		border-radius: 8px;
		overflow: hidden;
		background: var(--bg-sunk);
		margin-bottom: 10px;
	}
	.bar span {
		display: block;
		height: 100%;
		background: var(--accent-ink);
	}
	.definition,
	.notice {
		margin-top: 16px;
		max-width: 100ch;
	}
	@media (max-width: 600px) {
		.intro {
			display: block;
		}
		.intro p {
			margin-top: 10px;
		}
		.metrics {
			grid-template-columns: 1fr;
			gap: 16px;
		}
		.metrics > div {
			display: flex;
			align-items: baseline;
			justify-content: space-between;
			gap: 12px;
		}
		dd {
			flex: none;
		}
	}
</style>
