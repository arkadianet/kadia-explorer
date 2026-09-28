<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { api } from '$lib/api/endpoints';
	import type { BoxDto, TxDto } from '$lib/api/types';
	import { formatErg, formatNano } from '$lib/format/amount';
	import { truncateMiddle } from '$lib/format/hash';
	import {
		createInvestigation,
		emptyGraph,
		seedRef,
		keyOf,
		references,
		edgeObserved,
		encodePlan,
		exportEvidence,
		MAX_NODES,
		MAX_EDGES,
		type Plan,
		type Node,
		type Kind
	} from '$lib/investigation/graph';
	let { initial, initialError }: { initial: Plan; initialError: string | null } = $props();
	const uid = $props.id();
	let graph = $state(emptyGraph());
	const scopedFetch =
		(signal?: AbortSignal): typeof fetch =>
		(input, init) =>
			fetch(input, {
				...init,
				signal:
					signal && init?.signal ? AbortSignal.any([signal, init.signal]) : (signal ?? init?.signal)
			});
	const controller = createInvestigation({
		tx: (id, signal) => api.tx(id, scopedFetch(signal)),
		box: (id, signal) => api.box(id, scopedFetch(signal)),
		onChange: (next) => (graph = next)
	});
	let kind = $state<Kind>('tx');
	let identifier = $state('');
	let formError = $state('');
	let initialErrorDismissed = $state(false);
	let actionMessage = $state('');
	let shareLink = $state('');
	let referenceLimit = $state(24);
	let view = $state<'graph' | 'list'>('graph');
	$effect(() => {
		const plan = initial;
		untrack(() => {
			controller.restore(plan);
			kind = plan.nodes[0]?.kind ?? 'tx';
			identifier = plan.nodes[0]?.id ?? '';
			formError = '';
			initialErrorDismissed = false;
			actionMessage = '';
			shareLink = '';
		});
	});
	$effect(() => {
		void graph.selected;
		referenceLimit = 24;
	});
	$effect(() => {
		void graph.nodes;
		void graph.edges;
		shareLink = '';
	});
	onDestroy(() => controller.stop());
	const selected = $derived(graph.nodes.find((n) => n.key === graph.selected));
	const selectedTx = $derived(
		selected?.kind === 'tx' && selected.data ? (selected.data as TxDto) : null
	);
	const selectedBox = $derived(
		selected?.kind === 'box' && selected.data ? (selected.data as BoxDto) : null
	);
	const connections = $derived(selected ? references(selected) : []);
	const loaded = $derived(graph.nodes.filter((n) => n.data).length);
	const stale = $derived(graph.nodes.some((n) => n.status === 'stale'));
	const layout = $derived.by(() => {
		const depth: Record<string, number> = {};
		const seed = graph.nodes[0];
		if (seed) depth[seed.key] = 0;
		for (let pass = 0; pass < MAX_NODES; pass++)
			for (const edge of graph.edges) {
				if (depth[edge.from] !== undefined && depth[edge.to] === undefined)
					depth[edge.to] = depth[edge.from] + 1;
				if (depth[edge.to] !== undefined && depth[edge.from] === undefined)
					depth[edge.from] = depth[edge.to] + 1;
			}
		const columns: Record<number, number> = {};
		const positions: Record<string, { x: number; y: number }> = {};
		for (const node of graph.nodes) {
			const column = Math.min(depth[node.key] ?? 0, 4);
			const row = columns[column] ?? 0;
			columns[column] = row + 1;
			positions[node.key] = { x: 30 + column * 230, y: 40 + row * 112 };
		}
		return {
			positions,
			width: Math.max(710, 260 + Math.max(0, ...Object.keys(columns).map(Number)) * 230),
			height: Math.max(310, 60 + Math.max(0, ...Object.values(columns)) * 112)
		};
	});
	function line(from: string, to: string) {
		const a = layout.positions[from],
			b = layout.positions[to];
		if (!a || !b) return '';
		const forward = b.x > a.x;
		const ax = a.x + (forward ? 190 : 0),
			bx = b.x + (forward ? 0 : 190);
		return `M${ax},${a.y + 40} C${ax + (forward ? 70 : -70)},${a.y + 40} ${bx + (forward ? -70 : 70)},${b.y + 40} ${bx},${b.y + 40}`;
	}
	function entityUrl(node: Node) {
		return `/${node.kind === 'tx' ? 'tx' : 'box'}/${node.id}`;
	}
	async function start(event?: SubmitEvent) {
		event?.preventDefault();
		const seed = seedRef(kind, identifier);
		if (!seed) {
			formError = 'Enter a 64-character hexadecimal transaction or box ID.';
			return;
		}
		formError = '';
		initialErrorDismissed = true;
		shareLink = '';
		actionMessage = '';
		controller.restore({ nodes: [seed], edges: [] });
		await controller.load(keyOf(seed));
	}
	async function share() {
		try {
			shareLink = `${location.origin}/investigate?plan=${encodePlan(graph)}`;
			await navigator.clipboard.writeText(shareLink);
			actionMessage = 'Investigation link copied. Recipients load each node explicitly.';
		} catch (error) {
			actionMessage = shareLink
				? 'The link is ready below; clipboard access was unavailable.'
				: error instanceof Error
					? error.message
					: 'Unable to create a link.';
		}
	}
	function download() {
		try {
			const raw = exportEvidence(graph);
			const url = URL.createObjectURL(new Blob([raw], { type: 'application/json;charset=utf-8' }));
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = `kadia-investigation-${new Date().toISOString().slice(0, 10)}.json`;
			anchor.click();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			actionMessage =
				'Exported exact loaded evidence, read times and connection states. Unloaded nodes remain marked as unloaded.';
		} catch (error) {
			actionMessage = error instanceof Error ? error.message : 'Unable to export evidence.';
		}
	}
</script>

<section class="investigation">
	<header class="investigation-intro">
		<div>
			<p class="eyebrow">Evidence / one connection at a time</p>
			<h1>Follow the references.</h1>
			<p class="lead">
				Build an investigation from a transaction or box. Open only the connections you want to
				inspect.
			</p>
		</div>
		<div class="scope-note">
			<strong>Membership, not attribution.</strong>
			<p>
				Connections describe indexed box inputs, outputs and data reads. They do not identify owners
				or allocate value between particular inputs and outputs.
			</p>
		</div>
	</header>
	<form class="seed-form" onsubmit={(event) => void start(event)}>
		<div>
			<label for={`${uid}-kind`}>Start from</label><select
				id={`${uid}-kind`}
				class="control"
				bind:value={kind}
				><option value="tx">Transaction</option><option value="box">Box</option></select
			>
		</div>
		<div class="seed-id">
			<label for={`${uid}-id`}>Transaction or box ID</label><input
				id={`${uid}-id`}
				class="control mono"
				bind:value={identifier}
				autocomplete="off"
				spellcheck="false"
				placeholder="64 hexadecimal characters"
				maxlength="64"
			/>
		</div>
		<button class="primary" type="submit"
			>{graph.nodes.length ? 'Start a fresh investigation' : 'Load seed'}</button
		>
	</form>
	{#if (!initialErrorDismissed && initialError) || formError}<p class="notice danger" role="alert">
			{formError || initialError}
		</p>{/if}
	<p class="read-scope">
		Each expansion makes one explicit detail request. Reads may come from different indexed tips and
		are not a common canonical snapshot. Box creation height is not transaction inclusion height.
	</p>
	{#if graph.conflict}<div class="notice danger" role="alert">
			<strong>Conflicting indexed evidence</strong>
			<p>{graph.conflict}</p>
			<button
				class="text-button"
				onclick={() => {
					const seed = graph.nodes[0];
					if (seed) {
						kind = seed.kind;
						identifier = seed.id;
						void start();
					}
				}}>Restart from seed without old pins</button
			>
		</div>{/if}
	{#if graph.notice}<p class="notice" role="status">{graph.notice}</p>{/if}
	{#if stale}<p class="notice" role="status">
			A refresh failed. Older evidence remains marked stale; refresh it before expanding its
			connections.
		</p>{/if}
	{#if graph.nodes.length}
		<div class="investigation-toolbar">
			<p>
				<strong>{loaded}</strong> loaded / {graph.nodes.length} selected
				<span>· {graph.edges.length} connections · limits {MAX_NODES}/{MAX_EDGES}</span>
			</p>
			<div class="actions">
				<button onclick={() => void share()} disabled={graph.busy || !!graph.conflict}
					>Copy investigation link</button
				><button onclick={download} disabled={!loaded || graph.busy || !!graph.conflict}
					>Export evidence JSON</button
				>
			</div>
		</div>
		{#if actionMessage}<p class="action-message" role="status">{actionMessage}</p>{/if}
		{#if shareLink}<label class="share-field"
				>Shareable plan<input
					class="control mono"
					value={shareLink}
					readonly
					onclick={(event) => event.currentTarget.select()}
				/><span
					>IDs, connections and observed transaction inclusion pins only. No background loading or
					assertion that a pinned block is still canonical.</span
				></label
			>{/if}
		{#if !loaded && !graph.busy && !graph.conflict}<p class="plan-note">
				This plan is local and unloaded. Select a node, then choose Load selected node. Planned
				connections are unverified until their evidence is read.
			</p>{/if}
		<div class="investigation-workspace">
			<div class="workspace-map">
				<div class="view-switch" role="group" aria-label="Investigation view">
					<button aria-pressed={view === 'graph'} onclick={() => (view = 'graph')}
						>Graph view</button
					><button aria-pressed={view === 'list'} onclick={() => (view = 'list')}
						>Evidence list</button
					>
				</div>
				{#if view === 'graph'}
					<!-- svelte-ignore a11y_no_noninteractive_tabindex (The bounded diagram must be scrollable from a keyboard.) -->
					<div
						class="graph-scroll"
						tabindex="0"
						role="region"
						aria-label="Investigation graph; scroll to explore"
					>
						<div class="graph-canvas" style={`width:${layout.width}px;height:${layout.height}px`}>
							<svg
								class="connections"
								width={layout.width}
								height={layout.height}
								aria-hidden="true"
								><defs
									><marker
										id={`${uid}-arrow`}
										viewBox="0 0 10 10"
										refX="9"
										refY="5"
										markerWidth="6"
										markerHeight="6"
										orient="auto-start-reverse"><path d="M0 0L10 5L0 10Z" /></marker
									></defs
								>{#each graph.edges as edge (`${edge.from}:${edge.to}:${edge.relation}`)}<path
										class:unverified={!edgeObserved(edge, graph.nodes)}
										class:read={edge.relation === 'reads'}
										d={line(edge.from, edge.to)}
										marker-end={`url(#${uid}-arrow)`}
									/>{/each}</svg
							>
							{#each graph.nodes as node (node.key)}<button
									class="graph-node"
									class:selected={graph.selected === node.key}
									class:transaction={node.kind === 'tx'}
									style={`left:${layout.positions[node.key].x}px;top:${layout.positions[node.key].y}px`}
									aria-label={`Inspect ${node.kind === 'tx' ? 'transaction' : 'box'} ${node.id}`}
									aria-pressed={graph.selected === node.key}
									onclick={() => controller.select(node.key)}
									><span class="node-kind">{node.kind === 'tx' ? 'Transaction' : 'Box'}</span
									><strong>{truncateMiddle(node.id, 10, 8)}</strong><small
										>{node.status === 'idle' ? 'Not loaded' : node.status}</small
									></button
								>{/each}
						</div>
					</div>
					<p class="graph-key">
						Arrows: box → spending/reading transaction; producing transaction → box. Dashed lines:
						unloaded plan. Dotted lines: data reads.
					</p>
				{:else}<ol class="evidence-list" aria-label="Investigation nodes">
						{#each graph.nodes as node, index (node.key)}<li>
								<button
									aria-pressed={graph.selected === node.key}
									onclick={() => controller.select(node.key)}
									><span
										>{String(index + 1).padStart(2, '0')} / {node.kind === 'tx'
											? 'Transaction'
											: 'Box'}</span
									><strong>{node.id}</strong><small
										>{node.status === 'idle' ? 'Not loaded' : node.status}{node.pin
											? ` · inclusion ${node.pin.height}`
											: ''}</small
									></button
								>
							</li>{/each}
					</ol>{/if}
				<details class="connection-list">
					<summary>Connection evidence ({graph.edges.length})</summary>
					<ul>
						{#each graph.edges as edge (`${edge.from}:${edge.to}:${edge.relation}`)}<li>
								<code>{edge.from}</code><strong>{edge.relation}</strong><code>{edge.to}</code><span
									>{edgeObserved(edge, graph.nodes)
										? 'Observed in loaded details'
										: 'Unverified shared plan'}</span
								>
							</li>{/each}
					</ul>
					{#if !graph.edges.length}<p>No connections selected yet.</p>{/if}
				</details>
			</div>
			<aside class="inspector" aria-label="Selected node inspector" aria-busy={graph.busy}>
				{#if selected}<header>
						<p class="eyebrow">{selected.kind === 'tx' ? 'Transaction' : 'Box'} inspector</p>
						<h2>{selected.data ? 'Indexed evidence' : 'Ready to inspect'}</h2>
						<code class="entity-id">{selected.id}</code><a
							class="entity-link"
							href={entityUrl(selected)}
							>Open {selected.kind === 'tx' ? 'transaction' : 'box'} details ↗</a
						>
					</header>
					{#if selected.pin}<p class="pin">
							Inclusion pin: {selected.pin.height.toLocaleString('en-US')}<a
								href={`/blocks/${selected.pin.block_id}`}><code>{selected.pin.block_id}</code></a
							>
						</p>{/if}
					<button
						class="primary node-load"
						disabled={graph.busy || !!graph.conflict}
						onclick={() => void controller.load(selected!.key)}
						>{selected.status === 'loading'
							? 'Loading selected node…'
							: selected.data
								? 'Refresh selected node'
								: 'Load selected node'}</button
					>
					{#if selected.status === 'loading'}<p class="loading" role="status">
							Reading this node only…
						</p>{/if}
					{#if selected.error}<p class="notice danger" role="alert">
							{selected.status === 'stale' ? 'Stale evidence. ' : ''}{selected.error}
						</p>{/if}
					{#if selected.checkedAt}<p class="checked">
							Read {new Date(selected.checkedAt).toISOString()} · no automatic refresh
						</p>{/if}
					{#if selectedTx}<dl class="facts">
							<div>
								<dt>Included at height</dt>
								<dd>{selectedTx.height.toLocaleString('en-US')}</dd>
							</div>
							<div>
								<dt>Detail indexed tip</dt>
								<dd>{selectedTx.indexed_height?.toLocaleString('en-US') ?? 'Not supplied'}</dd>
							</div>
							<div>
								<dt>Block identity</dt>
								<dd>
									{selectedTx.block_id
										? 'Available in inclusion pin'
										: 'Not supplied; inclusion is unpinned'}
								</dd>
							</div>
							<div>
								<dt>Fee</dt>
								<dd>
									{formatErg(selectedTx.fee)} ERG<small>{formatNano(selectedTx.fee)} nanoERG</small>
								</dd>
							</div>
							<div>
								<dt>Input coverage</dt>
								<dd>
									{selectedTx.inputs.filter((i) => i.box !== null).length} / {selectedTx.inputs
										.length} resolved
								</dd>
							</div>
						</dl>
						{#if selectedTx.inputs.some((i) => !i.box)}<p class="notice">
								Some input values and scripts are unresolved. Their IDs can be followed, but missing
								details do not establish balances or zero values.
							</p>{/if}
					{:else if selectedBox}<p class="box-amount">
							{formatErg(selectedBox.value)} <span>ERG</span>
						</p>
						<p class="raw-value">{formatNano(selectedBox.value)} nanoERG · exact box value</p>
						<dl class="facts">
							<div>
								<dt>Creation height</dt>
								<dd>
									{selectedBox.creation_height.toLocaleString('en-US')}<small
										>Not inclusion height</small
									>
								</dd>
							</div>
							<div>
								<dt>Spend observation</dt>
								<dd>
									{selectedBox.spent_by
										? `Indexed spend at ${selectedBox.spent_height}`
										: 'No indexed spend in this read'}
								</dd>
							</div>
							<div>
								<dt>Snapshot tip</dt>
								<dd>Not supplied by box endpoint</dd>
							</div>
							<div>
								<dt>Token IDs</dt>
								<dd>{selectedBox.tokens.length}</dd>
							</div>
						</dl>
						{#if selectedBox.tokens.length}<details class="box-tokens">
								<summary>Raw token quantities</summary>
								<ul>
									{#each selectedBox.tokens.slice(0, 20) as token (token.id)}<li>
											<a href={`/token/${token.id}`}><code>{token.id}</code></a><strong
												>{formatNano(token.amount)} raw units</strong
											>
										</li>{/each}
								</ul>
								{#if selectedBox.tokens.length > 20}<p>
										Showing 20 of {selectedBox.tokens.length}; all loaded quantities are included in
										the evidence export.
									</p>{/if}
							</details>{/if}
					{/if}
					{#if selected.data}<section class="reference-section">
							<h3>Follow a connection <span>{connections.length}</span></h3>
							<ul>
								{#each connections.slice(0, referenceLimit) as reference (`${reference.edge.relation}:${reference.ref.id}`)}<li
									>
										<span>{reference.label}</span><code>{reference.ref.id}</code
										>{#if !reference.resolved && reference.edge.relation !== 'reads'}<small
												>Unresolved in this transaction</small
											>{/if}<button
											disabled={graph.busy || !!graph.conflict || selected.status === 'stale'}
											onclick={() => void controller.expand(selected!.key, reference)}
											>Expand {reference.ref.kind === 'tx' ? 'transaction' : 'box'}
											{truncateMiddle(reference.ref.id, 6, 4)}</button
										>
									</li>{/each}
							</ul>
							{#if connections.length > referenceLimit}<button
									class="text-button"
									onclick={() => (referenceLimit += 24)}
									>Show 24 more references ({connections.length - referenceLimit} remaining)</button
								>{/if}{#if !connections.length}<p>
									No further indexed references. A genesis box has no producing transaction.
								</p>{/if}
						</section>{/if}
				{/if}
			</aside>
		</div>
	{:else}<div class="investigation-empty">
			<span aria-hidden="true">01 → 02 → 03</span>
			<h2>A path you choose.</h2>
			<p>
				Start with an ID. Inspect a box’s producing or spending transaction, then follow another
				box. The explorer never expands a branch automatically.
			</p>
			<p>Up to 24 nodes, 48 connections and 4 MiB of loaded evidence per investigation.</p>
		</div>{/if}
</section>

<style>
	.investigation {
		min-width: 0;
	}
	.investigation-intro {
		display: grid;
		grid-template-columns: minmax(0, 1.5fr) minmax(220px, 0.7fr);
		gap: 36px;
		align-items: end;
		margin-bottom: 28px;
	}
	.eyebrow {
		font: 11px/1.7 var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--accent-ink);
	}
	h1 {
		font: var(--weight-display, 700) clamp(34px, 5vw, 64px)/1.02
			var(--font-display, var(--font-sans));
		letter-spacing: -0.04em;
		margin: 12px 0 18px;
	}
	.lead {
		max-width: 64ch;
		font-size: 14px;
		line-height: 1.75;
		color: var(--fg-muted);
	}
	.scope-note {
		border-left: 2px solid var(--accent-ink);
		padding-left: 20px;
		font-size: 12px;
		line-height: 1.7;
	}
	.scope-note p {
		margin-top: 8px;
		color: var(--fg-muted);
	}
	.seed-form {
		display: flex;
		gap: 14px;
		align-items: end;
		padding: 22px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	.seed-form > div {
		min-width: 0;
	}
	.seed-id {
		flex: 1;
	}
	.seed-form label {
		display: block;
		font-size: 12px;
		font-weight: 650;
		margin-bottom: 8px;
	}
	.seed-form input,
	.seed-form select {
		width: 100%;
		min-width: 0;
	}
	.seed-form input {
		font-size: 12px;
	}
	button {
		cursor: pointer;
	}
	.primary {
		padding: 12px 16px;
		border: 1px solid var(--accent-ink);
		border-radius: var(--radius-control);
		background: var(--accent-ink);
		color: var(--surface-solid);
		font: 650 12px var(--font-sans);
		line-height: 1.5;
		max-width: 100%;
	}
	button:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.read-scope {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.8;
		margin: 16px 0 24px;
		max-width: 110ch;
	}
	.notice {
		border-left: 3px solid var(--warn-ink);
		padding: 14px 16px;
		background: var(--surface-hover);
		font-size: 12px;
		line-height: 1.75;
		margin: 16px 0;
		overflow-wrap: anywhere;
	}
	.notice p {
		margin-top: 8px;
	}
	.danger {
		border-color: var(--danger-ink);
		color: var(--danger-ink);
	}
	.text-button {
		padding: 10px 0;
		background: none;
		border: 0;
		color: var(--accent-ink);
		text-decoration: underline;
		font-size: 12px;
		text-align: left;
	}
	.investigation-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 18px;
		flex-wrap: wrap;
		padding: 18px 0;
		border-top: var(--rule);
	}
	.investigation-toolbar p {
		font: 12px var(--font-mono);
		line-height: 1.8;
	}
	.investigation-toolbar p > span {
		color: var(--fg-muted);
	}
	.actions {
		display: flex;
		gap: 10px;
		flex-wrap: wrap;
	}
	.actions button,
	.view-switch button {
		padding: 9px 12px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		font-size: 12px;
	}
	.action-message,
	.plan-note {
		font-size: 12px;
		line-height: 1.7;
		margin: 12px 0;
		color: var(--fg-muted);
	}
	.share-field {
		display: grid;
		gap: 8px;
		font-size: 12px;
		margin: 16px 0;
	}
	.share-field input {
		min-width: 0;
		width: 100%;
		font-size: 11px;
	}
	.share-field span {
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.investigation-workspace {
		display: grid;
		grid-template-columns: minmax(0, 1.5fr) minmax(300px, 0.9fr);
		gap: 24px;
		align-items: start;
	}
	.workspace-map {
		min-width: 0;
		overflow: hidden;
		border: var(--rule);
		background: var(--surface-solid);
		border-radius: var(--radius-card);
	}
	.view-switch {
		display: flex;
		gap: 8px;
		padding: 16px;
		border-bottom: var(--rule);
	}
	.view-switch button[aria-pressed='true'] {
		border-color: var(--accent-ink);
		color: var(--accent-ink);
		background: var(--accent-wash);
	}
	.graph-scroll {
		max-width: 100%;
		overflow: auto;
		max-height: 660px;
	}
	.graph-canvas {
		position: relative;
		background-image: radial-gradient(var(--hairline) 0.6px, transparent 0.6px);
		background-size: 22px 22px;
	}
	.connections {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.connections > path {
		fill: none;
		stroke: var(--accent-ink);
		stroke-width: 1.7;
	}
	.connections > path.unverified {
		stroke-dasharray: 7 5;
		opacity: 0.6;
	}
	.connections > path.read {
		stroke-dasharray: 2 4;
	}
	.connections marker path {
		fill: var(--accent-ink);
	}
	.graph-node {
		position: absolute;
		width: 190px;
		height: 80px;
		text-align: left;
		display: grid;
		gap: 5px;
		align-content: center;
		padding: 12px 15px;
		border: 1px solid var(--fg-muted);
		border-radius: 6px;
		background: var(--surface-solid);
		color: var(--fg);
		box-shadow: var(--shadow-rest);
	}
	.graph-node.transaction {
		border-radius: 18px;
		border-color: var(--accent-ink);
	}
	.graph-node.selected {
		outline: 3px solid var(--accent-ink);
		outline-offset: 3px;
	}
	.node-kind {
		font: 11px var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.graph-node strong {
		font: 12px var(--font-mono);
	}
	.graph-node small {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.graph-key {
		padding: 16px;
		font-size: 11px;
		color: var(--fg-muted);
		line-height: 1.75;
		border-top: var(--rule);
	}
	.evidence-list {
		padding: 16px;
		display: grid;
		gap: 10px;
		max-height: 660px;
		overflow: auto;
	}
	.evidence-list button {
		display: grid;
		gap: 8px;
		text-align: left;
		padding: 16px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		width: 100%;
		min-width: 0;
	}
	.evidence-list button[aria-pressed='true'] {
		border-color: var(--accent-ink);
		box-shadow: inset 3px 0 var(--accent-ink);
	}
	.evidence-list span,
	.evidence-list small {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.evidence-list strong {
		font: 11px/1.6 var(--font-mono);
		overflow-wrap: anywhere;
	}
	.connection-list {
		padding: 16px;
		border-top: var(--rule);
		font-size: 12px;
	}
	.connection-list summary,
	.box-tokens summary {
		cursor: pointer;
	}
	.connection-list li {
		display: grid;
		gap: 6px;
		padding: 16px 0;
		border-bottom: var(--rule);
	}
	code {
		font-size: 11px;
		line-height: 1.65;
		overflow-wrap: anywhere;
	}
	.connection-list strong {
		color: var(--accent-ink);
		font-size: 11px;
	}
	.connection-list span {
		font-size: 11px;
		color: var(--fg-muted);
	}
	.connection-list p {
		margin-top: 12px;
		color: var(--fg-muted);
	}
	.inspector {
		min-width: 0;
		padding: 24px;
		border: var(--rule);
		background: var(--surface-solid);
		border-radius: var(--radius-card);
	}
	h2 {
		font: var(--weight-display, 650) 26px/1.15 var(--font-display, var(--font-sans));
		letter-spacing: -0.025em;
		margin: 10px 0 16px;
	}
	.entity-id {
		display: block;
		color: var(--fg-muted);
	}
	.entity-link {
		display: inline-block;
		color: var(--accent-ink);
		font-size: 12px;
		margin-top: 12px;
	}
	.pin {
		font-size: 11px;
		line-height: 1.7;
		margin-top: 18px;
		padding-top: 14px;
		border-top: var(--rule);
	}
	.pin a {
		display: block;
		color: var(--accent-ink);
	}
	.node-load {
		margin: 20px 0 6px;
		width: 100%;
	}
	.checked,
	.loading {
		font: 11px/1.75 var(--font-mono);
		color: var(--fg-muted);
		margin: 12px 0;
		overflow-wrap: anywhere;
	}
	.facts {
		margin-top: 20px;
	}
	.facts > div {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 20px;
		padding: 13px 0;
		border-top: var(--rule);
	}
	dt {
		font-size: 11px;
		color: var(--fg-muted);
		flex-shrink: 0;
	}
	dd {
		font: 12px/1.7 var(--font-number, var(--font-mono));
		text-align: right;
		overflow-wrap: anywhere;
	}
	dd small {
		display: block;
		font: 11px/1.6 var(--font-sans);
		color: var(--fg-muted);
		margin-top: 4px;
	}
	.box-amount {
		font: 32px/1.25 var(--font-number, var(--font-mono));
		letter-spacing: -0.035em;
		margin: 24px 0 8px;
		overflow-wrap: anywhere;
	}
	.box-amount span {
		font-size: 14px;
		color: var(--fg-muted);
	}
	.raw-value {
		font: 11px/1.6 var(--font-mono);
		color: var(--fg-muted);
		overflow-wrap: anywhere;
	}
	.box-tokens {
		font-size: 12px;
		margin: 20px 0;
	}
	.box-tokens li {
		display: grid;
		gap: 8px;
		padding: 14px 0;
		border-bottom: var(--rule);
	}
	.box-tokens strong {
		font: 12px var(--font-mono);
		overflow-wrap: anywhere;
	}
	.box-tokens a {
		color: var(--accent-ink);
	}
	.box-tokens p {
		font-size: 11px;
		line-height: 1.7;
		margin-top: 12px;
		color: var(--fg-muted);
	}
	.reference-section {
		margin-top: 28px;
		padding-top: 20px;
		border-top: var(--rule);
	}
	h3 {
		font-size: 14px;
		margin-bottom: 16px;
	}
	h3 span {
		color: var(--fg-muted);
		font: 11px var(--font-mono);
		margin-left: 8px;
	}
	.reference-section li {
		padding: 15px 0;
		border-top: var(--rule);
		display: grid;
		gap: 7px;
	}
	.reference-section li > span {
		font-size: 11px;
		font-weight: 650;
	}
	.reference-section code {
		color: var(--fg-muted);
	}
	.reference-section small {
		font-size: 11px;
		color: var(--warn-ink);
	}
	.reference-section li button {
		padding: 9px 12px;
		background: var(--accent-wash);
		border: 1px solid var(--accent-ink);
		border-radius: var(--radius-control);
		color: var(--accent-ink);
		text-align: left;
		font-size: 11px;
	}
	.reference-section p {
		font-size: 12px;
		color: var(--fg-muted);
		line-height: 1.7;
	}
	.investigation-empty {
		padding: 60px 32px;
		border: var(--rule);
		border-radius: var(--radius-card);
		text-align: center;
		background: var(--surface-solid);
	}
	.investigation-empty > span {
		font: 16px var(--font-mono);
		color: var(--accent-ink);
	}
	.investigation-empty h2 {
		font-size: 40px;
	}
	.investigation-empty p {
		max-width: 65ch;
		margin: 16px auto;
		font-size: 13px;
		color: var(--fg-muted);
		line-height: 1.8;
	}
	:global(:root[data-appearance='prism']) .investigation-workspace {
		grid-template-columns: minmax(0, 1.8fr) minmax(300px, 0.8fr);
		gap: 0;
		border: var(--rule);
		border-radius: 20px;
		overflow: hidden;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='prism']) .workspace-map {
		border: 0;
		border-radius: 0;
	}
	:global(:root[data-appearance='prism']) .inspector {
		border: 0;
		border-left: var(--rule);
		border-radius: 0;
		min-height: 100%;
	}
	:global(:root[data-appearance='prism']) .graph-canvas {
		background-color: var(--accent-wash);
	}
	:global(:root[data-appearance='prism']) .graph-node {
		box-shadow: var(--shadow-lift);
		border-radius: 12px;
	}
	:global(:root[data-appearance='prism']) .graph-node.transaction {
		border-radius: 24px;
	}
	:global(:root[data-appearance='prism']) .seed-form {
		border-left: 4px solid var(--accent-ink);
	}
	:global(:root[data-appearance='atelier']) .investigation-intro {
		grid-template-columns: minmax(0, 1fr);
		gap: 20px;
		border-top: 3px double var(--fg);
		padding-top: 28px;
	}
	:global(:root[data-appearance='atelier']) .scope-note {
		max-width: 72ch;
		border-left: 0;
		padding: 0;
	}
	:global(:root[data-appearance='atelier']) .seed-form {
		border: 0;
		border-top: var(--rule);
		border-bottom: var(--rule);
		padding: 22px 0;
		border-radius: 0;
		background: none;
	}
	:global(:root[data-appearance='atelier']) .investigation-workspace {
		grid-template-columns: minmax(250px, 0.7fr) minmax(0, 1.4fr);
	}
	:global(:root[data-appearance='atelier']) .inspector {
		grid-column: 1;
		grid-row: 1;
		background: none;
		border: 0;
		border-right: var(--rule);
		border-radius: 0;
		padding: 0 28px 0 0;
	}
	:global(:root[data-appearance='atelier']) .workspace-map {
		grid-column: 2;
		grid-row: 1;
		border-radius: 0;
		background: none;
	}
	:global(:root[data-appearance='atelier']) .graph-canvas {
		background-image: none;
	}
	:global(:root[data-appearance='atelier']) .graph-node {
		box-shadow: none;
		border-radius: 0;
	}
	:global(:root[data-appearance='atelier']) .graph-node.transaction {
		border-radius: 0;
		border-width: 2px;
	}
	:global(:root[data-appearance='atelier']) .primary {
		background: none;
		color: var(--fg);
		border: 0;
		border-bottom: 2px solid var(--fg);
		border-radius: 0;
	}
	:global(:root[data-appearance='atelier']) .investigation-empty {
		background: none;
		border: 0;
		border-bottom: 3px double var(--fg);
		border-radius: 0;
	}
	:global(:root[data-appearance='aurora']) .investigation-intro {
		display: block;
		text-align: center;
		padding: 38px 30px;
		background: var(--hero-bg);
		color: var(--hero-fg);
		border-radius: 28px;
	}
	:global(:root[data-appearance='aurora']) .lead {
		margin: auto;
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='aurora']) .investigation-intro .eyebrow {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='aurora']) .scope-note {
		max-width: 70ch;
		margin: 26px auto 0;
		border: 0;
		padding: 0;
	}
	:global(:root[data-appearance='aurora']) .scope-note p {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='aurora']) .investigation-workspace {
		grid-template-columns: minmax(0, 1fr);
	}
	:global(:root[data-appearance='aurora']) .workspace-map {
		border-radius: 24px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .inspector {
		border-radius: 24px;
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 0 36px;
		padding: 32px;
	}
	:global(:root[data-appearance='aurora']) .inspector header {
		grid-column: 1;
	}
	:global(:root[data-appearance='aurora']) .inspector .reference-section {
		grid-column: 2;
		grid-row: 1 / span 12;
		margin: 0;
		padding: 0 0 0 30px;
		border: 0;
		border-left: var(--rule);
	}
	:global(:root[data-appearance='aurora']) .graph-node {
		border-radius: 16px;
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='aurora']) .graph-node.transaction {
		border-radius: 28px;
	}
	@media (max-width: 1050px) {
		.investigation-workspace,
		:global(:root[data-appearance='prism']) .investigation-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='prism']) .inspector {
			border-left: 0;
			border-top: var(--rule);
		}
		.investigation-intro {
			grid-template-columns: minmax(0, 1fr);
			gap: 20px;
		}
		.seed-form {
			flex-wrap: wrap;
		}
		.seed-id {
			min-width: 240px !important;
		}
		:global(:root[data-appearance='atelier']) .investigation-workspace {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(:root[data-appearance='atelier']) .inspector,
		:global(:root[data-appearance='atelier']) .workspace-map {
			grid-column: auto;
			grid-row: auto;
		}
		:global(:root[data-appearance='atelier']) .inspector {
			border: 0;
			border-top: var(--rule);
			padding: 24px 0 0;
		}
	}
	@media (max-width: 650px) {
		:global(:root[data-appearance='aurora']) .inspector {
			display: block;
			padding: 22px;
		}
		:global(:root[data-appearance='aurora']) .inspector .reference-section {
			border: 0;
			border-top: var(--rule);
			padding: 24px 0 0;
			margin-top: 24px;
		}
		.seed-form {
			display: grid;
			grid-template-columns: minmax(0, 1fr);
		}
		.seed-id {
			min-width: 0 !important;
		}
		.seed-form .primary {
			width: 100%;
		}
		.seed-form,
		.inspector {
			padding: 18px;
		}
		.facts > div {
			gap: 14px;
		}
		dt {
			flex-shrink: 1;
		}
		.investigation-empty {
			padding: 36px 20px;
		}
		:global(:root[data-appearance='aurora']) .investigation-intro {
			padding: 28px 20px;
		}
		.actions {
			width: 100%;
		}
		.actions button {
			flex: 1;
		}
		.investigation-toolbar p > span {
			display: block;
		}
		.graph-scroll {
			max-height: 440px;
		}
		.node-kind {
			font-size: 11px;
		}
	}
</style>
