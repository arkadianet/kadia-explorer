<script lang="ts">
	import type { BoxDto, TxDto } from '$lib/api/types';
	import type { AddressEffect } from '$lib/tx/effects';
	import { formatErg, formatTokenAmount } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import Hash from './Hash.svelte';
	import { appearance } from '$lib/theme/theme.svelte';

	let {
		tx,
		addresses,
		selected,
		selectedLabel,
		complete,
		missingInputs,
		onselect
	}: {
		tx: TxDto;
		addresses: AddressEffect[];
		selected: AddressEffect | undefined;
		selectedLabel: string;
		complete: boolean;
		missingInputs: number;
		onselect: (tree: string) => void;
	} = $props();

	let inputLimit = $state(4);
	let outputLimit = $state(4);
	let stage: HTMLDivElement;
	let beamPaths = $state<string[]>([]);
	let beamSize = $state({ width: 1, height: 1 });
	const spatial = $derived(appearance.current === 'prism');
	const selectable = $derived(new Set(addresses.map((address) => address.tree)));
	const sides = $derived([
		{ key: 'input', label: 'Inputs', boxes: tx.inputs, limit: inputLimit },
		{
			key: 'output',
			label: 'Outputs',
			boxes: tx.outputs.map((box) => ({ id: box.id, box })),
			limit: outputLimit
		}
	]);

	function canSelect(box: BoxDto) {
		return box.kind !== 'fee' && selectable.has(box.tree_hash);
	}
	function signedErg(value: bigint) {
		return `${value > 0n ? '+' : ''}${formatErg(value)}`;
	}

	$effect(() => {
		if (!spatial || !stage) return;
		void inputLimit;
		void outputLimit;
		void tx.id;
		let frame = 0;
		function draw() {
			const bounds = stage.getBoundingClientRect();
			const hub = stage.querySelector('.crystal')!.getBoundingClientRect();
			if (!bounds.width || !bounds.height) return;
			const centerX = hub.x + hub.width / 2 - bounds.x;
			const centerY = hub.y + hub.height / 2 - bounds.y;
			beamSize = { width: bounds.width, height: bounds.height };
			beamPaths = Array.from(stage.querySelectorAll('.flow-box')).map((box) => {
				const rect = box.getBoundingClientRect();
				const input = !!box.closest('.inputs');
				const x = (input ? rect.right : rect.left) - bounds.x;
				const y = rect.top + rect.height / 2 - bounds.y;
				const bend = (x + centerX) / 2;
				return `M${x},${y} C${bend},${y} ${bend},${centerY} ${centerX},${centerY}`;
			});
		}
		function schedule() {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(draw);
		}
		const observer = new ResizeObserver(schedule);
		observer.observe(stage);
		for (const box of stage.querySelectorAll('.flow-box, .crystal')) observer.observe(box);
		schedule();
		return () => {
			observer.disconnect();
			cancelAnimationFrame(frame);
		};
	});
</script>

<section class="transaction-flow" aria-label="Transaction box flow">
	<header class="flow-intro">
		<div>
			<p class="flow-kicker">Follow the boxes</p>
			<h3>
				{#if spatial}Follow every <span>movement.</span>{:else}One transaction. Every side.{/if}
			</h3>
		</div>
		<p>Select an address to inspect its net change. Lines show transaction topology, not value.</p>
	</header>

	<div class="flow-stage" bind:this={stage}>
		{#if spatial}
			<svg
				class="spatial-beams"
				viewBox={`0 0 ${beamSize.width} ${beamSize.height}`}
				preserveAspectRatio="none"
				aria-hidden="true"
			>
				<g class="beam-glow"
					>{#each beamPaths as path, i (i)}<path d={path} />{/each}</g
				>
				<g
					>{#each beamPaths as path, i (i)}<path d={path} />{/each}</g
				>
			</svg>
		{/if}
		{#each sides as side (side.key)}
			<section
				class="flow-side"
				class:inputs={side.key === 'input'}
				class:outputs={side.key === 'output'}
				aria-label={side.label}
			>
				<header class="side-heading">
					<h4>{side.label}</h4>
					<span
						>{side.boxes.length.toLocaleString('en-US')}
						{side.boxes.length === 1 ? 'box' : 'boxes'}</span
					>
				</header>
				<div class="box-list">
					{#each side.boxes.slice(0, side.limit) as item, index (item.id)}
						{@const box = item.box}
						<article
							class="flow-box"
							class:selected={box && canSelect(box) && selected?.tree === box.tree_hash}
							class:unresolved={!box}
							aria-label={`${side.key === 'input' ? 'Input' : 'Output'} ${index + 1} box`}
						>
							{#if box && canSelect(box)}
								<button
									type="button"
									class="box-select"
									aria-label={`Select ${side.key} ${index + 1} address`}
									aria-pressed={selected?.tree === box.tree_hash}
									onclick={() => onselect(box.tree_hash)}
								>
									<span>{side.key === 'input' ? 'Input' : 'Output'} {index + 1}</span>
									<span class="selection-label"
										>{selected?.tree === box.tree_hash
											? 'Selected address'
											: 'Select address'}</span
									>
								</button>
							{:else}
								<p class="box-label">
									{side.key === 'input' ? 'Input' : 'Output'}
									{index + 1}<span>{box?.kind === 'fee' ? 'Miner fee contract' : 'Unresolved'}</span
									>
								</p>
							{/if}
							{#if box}
								<p class="box-amount">{formatErg(box.value)} <span>ERG</span></p>
								{#if box.tokens.length}
									<ul class="box-tokens" aria-label="Box tokens">
										{#each box.tokens.slice(0, 3) as token (token.id)}
											<li>
												<span class="token-quantity"
													>{formatTokenAmount(token.amount, token.decimals)}</span
												>
												<a href={`/token/${token.id}`}>{token.name?.trim() || 'Unnamed token'}</a
												><small title={token.id}
													>{truncateMiddle(token.id)}{token.decimals === null
														? ' · raw units; decimals unknown'
														: ''}</small
												>
											</li>
										{/each}
									</ul>
									{#if box.tokens.length > 3}
										<details class="more-tokens">
											<summary>{box.tokens.length - 3} more tokens</summary>
											<ul class="box-tokens">
												{#each box.tokens.slice(3) as token (token.id)}
													<li>
														<span class="token-quantity"
															>{formatTokenAmount(token.amount, token.decimals)}</span
														>
														<a href={`/token/${token.id}`}
															>{token.name?.trim() || 'Unnamed token'}</a
														><small title={token.id}
															>{truncateMiddle(token.id)}{token.decimals === null
																? ' · raw units; decimals unknown'
																: ''}</small
														>
													</li>
												{/each}
											</ul>
										</details>
									{/if}
								{/if}
								<p class="box-address" title={box.address ?? box.tree_hash}>
									{box.kind === 'fee'
										? 'Miner fee contract'
										: box.kind === 'emission'
											? 'Emission contract'
											: 'Address'} · {truncateMiddle(box.address ?? box.tree_hash, 7, 5)}
								</p>
							{:else}
								<p class="unknown">Value and tokens unknown</p>
								<p class="box-address">This input is outside the indexed data.</p>
							{/if}
							<a class="box-link" href={`/box/${item.id}`} title={item.id}
								>Box {truncateMiddle(item.id, 7, 5)} <span aria-hidden="true">↗</span></a
							>
						</article>
					{/each}
				</div>
				{#if side.boxes.length === 0}<p class="empty-side">No {side.key} boxes</p>{/if}
				{#if side.boxes.length > 4}
					<div class="box-controls">
						<p>Showing {Math.min(side.limit, side.boxes.length)} of {side.boxes.length}</p>
						{#if side.limit < side.boxes.length}<button
								type="button"
								class="show-boxes"
								onclick={() => (side.key === 'input' ? (inputLimit += 4) : (outputLimit += 4))}
								>Show more {side.label.toLowerCase()}</button
							>{/if}
						{#if side.limit > 4}<button
								type="button"
								class="show-boxes"
								onclick={() => (side.key === 'input' ? (inputLimit = 4) : (outputLimit = 4))}
								>Show fewer {side.label.toLowerCase()}</button
							>{/if}
					</div>
				{/if}
			</section>
			{#if side.key === 'input'}
				<div class="flow-hub">
					<div class="crystal-shadow" aria-hidden="true"></div>
					<div class="crystal" aria-hidden="true"><span>Σ</span></div>
					<p class="hub-label">Transaction</p>
					<p class="hub-id" title={tx.id}>{truncateMiddle(tx.id, 6, 4)}</p>
					<p class="hub-fee"><span>Transaction fee</span>{formatErg(tx.fee)} ERG</p>
				</div>
			{/if}
		{/each}
	</div>
	{#if tx.data_inputs.length}
		<p class="flow-note">
			{tx.data_inputs.length} data input{tx.data_inputs.length === 1 ? '' : 's'} read by this transaction;
			data inputs are not spent and are not shown in this flow.
		</p>
	{/if}

	<section class="flow-inspector" aria-label="Selected address balance changes" aria-live="polite">
		<header>
			<p class="flow-kicker">Address inspector</p>
			<h3>
				{complete && selected ? `${selectedLabel} · net change` : 'Exact net changes unavailable'}
			</h3>
		</header>
		{#if !complete}
			<p class="inspector-note">
				{missingInputs} unresolved input box{missingInputs === 1 ? '' : 'es'}. Every input must be
				resolved before any address net change can be shown. Known box amounts above are individual
				box values.
			</p>
		{:else if selected}
			<div class="inspector-address">
				<Hash
					value={selected.address ?? selected.tree}
					href={selected.address ? `/address/${selected.address}` : undefined}
					head={10}
				/>
			</div>
			<div class="inspector-balances">
				<p class="net-erg" class:negative={selected.erg < 0n}>
					<span class="net-label">Net ERG change</span>
					<strong>{signedErg(selected.erg)}</strong> ERG
				</p>
				{#if selected.tokens.length}<div class="net-token-group">
						<p class="net-label">Net token change</p>
						<ul class="net-tokens">
							{#each selected.tokens as token (token.id)}
								<li>
									<strong class:negative={token.amount < 0n}
										>{token.amount > 0n ? '+' : ''}{formatTokenAmount(
											token.amount.toString(),
											token.decimals
										)}</strong
									> <a href={`/token/${token.id}`}>{token.name?.trim() || 'Unnamed token'}</a><small
										title={token.id}
										>{truncateMiddle(token.id)}{token.decimals === null
											? ' · raw units; decimals unknown'
											: ''}</small
									>
								</li>
							{/each}
						</ul>
					</div>{/if}
			</div>
			<p class="inspector-note">
				{selected.inputs} input{selected.inputs === 1 ? '' : 's'} · {selected.outputs} output{selected.outputs ===
				1
					? ''
					: 's'} for this locking script. Changes include returned change; the transaction fee is already
				reflected across address changes.
			</p>
		{:else}<p class="inspector-note">No non-fee address is available to inspect.</p>{/if}
	</section>
	<p class="flow-note">
		Address balances are grouped by locking script. This view does not establish ownership or
		identify an operator.
	</p>
</section>

<style>
	.transaction-flow {
		min-width: 0;
		padding-top: 8px;
	}
	.spatial-beams {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		z-index: 0;
		fill: none;
		stroke: var(--accent-ink);
		stroke-width: 1.5px;
		opacity: 0.65;
	}
	.beam-glow {
		filter: blur(4px);
		stroke-width: 7px;
		opacity: 0.3;
	}
	.net-label {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
		margin-bottom: 7px;
		font-family: var(--font-sans);
		font-weight: 500;
	}
	.net-token-group {
		min-width: 0;
	}
	.flow-intro {
		display: flex;
		justify-content: space-between;
		align-items: end;
		gap: 20px;
		margin-bottom: 24px;
	}
	.flow-intro > p {
		max-width: 34ch;
		font-size: 12px;
		color: var(--fg-muted);
	}
	.flow-kicker {
		color: var(--accent-ink);
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}
	h3 {
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 700);
		font-size: clamp(24px, 3vw, 34px);
		letter-spacing: -0.035em;
		line-height: 1.12;
		margin-top: 6px;
	}
	.flow-stage {
		position: relative;
		isolation: isolate;
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(100px, 0.55fr) minmax(0, 1fr);
		align-items: start;
		gap: 18px;
		padding: 24px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background:
			radial-gradient(ellipse at 50% 35%, var(--accent-wash), transparent 65%), var(--bg-sunk);
	}
	.flow-side {
		position: relative;
		min-width: 0;
		z-index: 1;
	}
	.inputs {
		grid-column: 1;
		grid-row: 1;
	}
	.outputs {
		grid-column: 3;
		grid-row: 1;
	}
	.side-heading {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: 6px;
		margin-bottom: 12px;
		font-size: 12px;
	}
	h4 {
		font-weight: 600;
		font-size: 13px;
	}
	.side-heading span {
		color: var(--fg-muted);
		font-family: var(--font-mono);
		font-size: 11px;
	}
	.box-list {
		display: grid;
		gap: 12px;
	}
	.flow-box {
		position: relative;
		min-width: 0;
		padding: 14px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		box-shadow: var(--shadow-rest);
	}
	.flow-box::after {
		content: '';
		position: absolute;
		top: 31px;
		width: 18px;
		height: 1px;
		background: var(--hairline);
		pointer-events: none;
	}
	.flow-box:not(:last-child)::before {
		content: '';
		position: absolute;
		top: 31px;
		bottom: -44px;
		width: 1px;
		background: var(--hairline);
		pointer-events: none;
	}
	.inputs .flow-box::before {
		right: -10px;
	}
	.outputs .flow-box::before {
		left: -10px;
	}
	.inputs .flow-box::after {
		right: -19px;
	}
	.outputs .flow-box::after {
		left: -19px;
	}
	.selected {
		border-color: var(--accent-ink);
		box-shadow:
			inset 3px 0 0 var(--accent-ink),
			var(--shadow-rest);
	}
	.unresolved {
		border-style: dashed;
	}
	.box-select,
	.box-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		text-align: left;
		font-size: 11px;
		color: var(--fg-muted);
	}
	.box-select {
		padding: 5px 0;
		margin: -5px 0 0;
		cursor: pointer;
		font: inherit;
		font-size: 11px;
		border: none;
		background: transparent;
		color: var(--accent-ink);
	}
	.box-select:hover {
		text-decoration: underline;
	}
	.selection-label {
		text-align: right;
	}
	.box-label > span {
		text-align: right;
	}
	.box-amount {
		margin-top: 14px;
		font-family: var(--font-number, var(--font-sans));
		font-weight: var(--weight-number, 650);
		font-size: clamp(18px, 2vw, 23px);
		font-variant-numeric: tabular-nums;
		line-height: 1.25;
		overflow-wrap: anywhere;
	}
	.box-amount > span {
		color: var(--fg-muted);
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 500;
	}
	.box-tokens,
	.net-tokens {
		list-style: none;
		padding: 0;
		margin: 10px 0 0;
		display: grid;
		gap: 9px;
	}
	.box-tokens li {
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	.token-quantity {
		font-variant-numeric: tabular-nums;
	}
	small {
		display: block;
		margin-top: 3px;
		color: var(--fg-muted);
		font-family: var(--font-mono);
		font-size: 11px;
		overflow-wrap: anywhere;
	}
	.more-tokens {
		margin-top: 10px;
		font-size: 11px;
	}
	summary {
		cursor: pointer;
		color: var(--accent-ink);
	}
	.box-address {
		color: var(--fg-muted);
		font-size: 11px;
		margin-top: 14px;
		overflow-wrap: anywhere;
	}
	.box-link {
		display: block;
		border-top: var(--rule);
		padding-top: 9px;
		margin-top: 9px;
		font-family: var(--font-mono);
		font-size: 11px;
		overflow-wrap: anywhere;
	}
	.box-link span {
		float: right;
	}
	.unknown {
		font-size: 14px;
		margin-top: 16px;
	}
	.box-controls {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px 12px;
		margin-top: 14px;
		font-size: 11px;
	}
	.box-controls p {
		width: 100%;
		color: var(--fg-muted);
	}
	.show-boxes {
		padding: 8px 10px;
		background: var(--surface-solid);
		color: var(--accent-ink);
		border: var(--rule);
		border-radius: var(--radius-control);
		font: inherit;
		cursor: pointer;
	}
	.show-boxes:hover {
		background: var(--surface-hover);
	}
	.empty-side {
		color: var(--fg-muted);
		font-size: 12px;
		padding: 20px 0;
	}
	.flow-hub {
		position: relative;
		grid-column: 2;
		grid-row: 1;
		align-self: start;
		min-width: 0;
		margin-top: 26px;
		text-align: center;
	}
	.flow-hub::before {
		content: '';
		position: absolute;
		z-index: -1;
		left: -18px;
		right: -18px;
		top: 36px;
		height: 1px;
		background: var(--hairline);
	}
	.crystal {
		position: relative;
		isolation: isolate;
		display: grid;
		place-items: center;
		width: 76px;
		height: 76px;
		margin: 0 auto;
		border: 1px solid var(--hairline);
		border-radius: 18px;
		color: var(--accent-ink);
		background: linear-gradient(135deg, var(--surface-solid), var(--accent-wash));
		box-shadow: var(--shadow-lift);
	}
	.crystal span {
		font-family: Georgia, serif;
		font-size: 44px;
		line-height: 1;
	}
	.crystal-shadow {
		display: none;
	}
	.hub-label {
		margin-top: 22px;
		font-size: 12px;
		font-weight: 650;
	}
	.hub-id {
		margin-top: 5px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.hub-fee {
		margin: 16px 0 0;
		padding: 12px 0 0;
		border-top: var(--rule);
		font-family: var(--font-mono);
		font-size: 11px;
		overflow-wrap: anywhere;
	}
	.hub-fee span {
		display: block;
		margin-bottom: 5px;
		font-family: var(--font-sans);
		color: var(--fg-muted);
	}
	.flow-inspector {
		margin-top: 22px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		box-shadow: var(--shadow-rest);
		padding: 22px;
	}
	.flow-inspector h3 {
		font-size: 25px;
	}
	.inspector-address {
		margin-top: 12px;
		font-size: 12px;
		min-width: 0;
	}
	.inspector-address :global(.hash) {
		max-width: 100%;
		overflow-wrap: anywhere;
	}
	.inspector-balances {
		display: flex;
		flex-wrap: wrap;
		gap: 20px 40px;
		align-items: start;
		margin-top: 20px;
	}
	.net-erg {
		font-size: 12px;
		color: var(--fg-muted);
		overflow-wrap: anywhere;
		max-width: 100%;
	}
	.net-erg strong {
		color: var(--accent-ink);
		font-family: var(--font-number, var(--font-sans));
		font-size: clamp(26px, 4vw, 38px);
		font-weight: var(--weight-number, 650);
		font-variant-numeric: tabular-nums;
	}
	.net-tokens {
		margin: 0;
		min-width: 0;
	}
	.net-tokens li {
		overflow-wrap: anywhere;
		font-size: 14px;
	}
	.net-tokens strong {
		font-size: 23px;
		font-family: var(--font-number, var(--font-sans));
		font-weight: var(--weight-number, 650);
		font-variant-numeric: tabular-nums;
		color: var(--accent-ink);
	}
	.net-erg.negative strong,
	.net-tokens strong.negative {
		color: var(--danger-ink);
	}
	.inspector-note,
	.flow-note {
		font-size: 12px;
		color: var(--fg-muted);
		margin-top: 16px;
	}
	@media (max-width: 760px) {
		.spatial-beams {
			display: none;
		}
		.flow-intro {
			display: block;
		}
		.flow-intro > p {
			max-width: none;
			margin-top: 12px;
		}
		.flow-stage {
			grid-template-columns: minmax(0, 1fr);
			padding: 16px;
			gap: 26px;
		}
		.inputs,
		.outputs,
		.flow-hub {
			grid-column: 1;
		}
		.inputs {
			grid-row: 1;
		}
		.flow-hub {
			grid-row: 2;
			margin: 0;
			padding: 4px 0;
		}
		.outputs {
			grid-row: 3;
		}
		.flow-hub::before {
			top: -26px;
			bottom: -26px;
			left: 50%;
			width: 1px;
			height: auto;
			z-index: -1;
		}
		.flow-hub > p {
			position: relative;
			width: fit-content;
			max-width: 100%;
			margin-left: auto;
			margin-right: auto;
			padding: 4px 10px;
			background: var(--bg-sunk);
		}
		.hub-fee {
			margin-top: 8px;
			border: none;
		}
		.flow-box::after,
		.flow-box:not(:last-child)::before {
			display: none;
		}
		.flow-inspector {
			padding: 16px;
		}
		:global(:root[data-appearance='prism']) .crystal {
			margin-top: 0;
		}
		:global(:root[data-appearance='prism']) .crystal-shadow {
			top: 112px;
		}
	}
</style>
