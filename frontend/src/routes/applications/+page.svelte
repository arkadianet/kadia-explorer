<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/endpoints';
	import Icon from '$lib/components/Icon.svelte';
	import ApplicationWorkflow from '$lib/components/ApplicationWorkflow.svelte';
	import RosenWorkflow from '$lib/components/RosenWorkflow.svelte';
	import { spectrumOrder, transactionOrders } from '$lib/tx/spectrum';
	import { rosenDeposit, transactionDeposits } from '$lib/apps/rosen';
	import {
		applicationSelector,
		applicationLink,
		createApplicationInspector,
		type InspectorState
	} from '$lib/apps/inspector';
	let kind = $state('tx'),
		id = $state(''),
		pin = $state(''),
		formError = $state('');
	let inspection = $state<InspectorState>({ loading: false, evidence: null, error: '' });
	let inspector: ReturnType<typeof createApplicationInspector> | undefined;
	$effect(() => {
		const params = page.url.searchParams;
		kind = params.get('kind') ?? 'tx';
		id = params.get('id') ?? '';
		pin = params.get('block') ?? '';
		formError = '';
		inspection = { loading: false, evidence: null, error: '' };
		const current = createApplicationInspector({
			getTx: api.tx,
			getBox: api.box,
			onChange: (next) => {
				inspection = next;
			}
		});
		inspector = current;
		return () => current.stop();
	});
	const orders = $derived.by(() => {
		const evidence = inspection.evidence;
		if (!evidence) return [];
		if (evidence.kind === 'tx') return transactionOrders(evidence.value);
		const order = spectrumOrder(evidence.value);
		return order ? [order] : [];
	});
	const deposits = $derived.by(() => {
		const evidence = inspection.evidence;
		if (!evidence) return [];
		if (evidence.kind === 'tx') return transactionDeposits(evidence.value);
		const deposit = rosenDeposit(evidence.value);
		return deposit ? [deposit] : [];
	});
	const share = $derived.by(() => {
		const evidence = inspection.evidence;
		if (!evidence) return '';
		return applicationLink({
			kind: evidence.kind,
			id: evidence.value.id,
			...(evidence.kind === 'tx' && evidence.value.block_id
				? { block: evidence.value.block_id }
				: {})
		});
	});
	function edit() {
		inspector?.reset();
		formError = '';
	}
	function load(event: SubmitEvent) {
		event.preventDefault();
		formError = '';
		try {
			const selector = applicationSelector(kind, id, pin);
			untrack(() => {
				void inspector?.load(selector);
			});
		} catch (cause) {
			formError = cause instanceof Error ? cause.message : 'Check the ID.';
		}
	}
</script>

<svelte:head
	><title>Application explorer — Kadia</title><meta
		name="description"
		content="Follow supported Spectrum orders and Rosen deposits through their indexed evidence."
	/></svelte:head
>

<section class="applications">
	<header class="app-intro">
		<div>
			<p class="eyebrow"><Icon name="layers" size={16} /> Applications / evidence first</p>
			<h1>One action.<br /><em>The whole trail.</em></h1>
		</div>
		<div class="intro-copy">
			<p>
				Follow supported swaps, liquidity orders and bridge deposits from the request to what the
				indexed evidence shows.
			</p>
			<a href="/investigate">Build a transaction investigation <span aria-hidden="true">↗</span></a>
		</div>
	</header>
	<form class="inspect-form" onsubmit={load} aria-label="Inspect application activity">
		<div>
			<label for="application-kind">Inspect a</label><select
				id="application-kind"
				bind:value={kind}
				onchange={() => {
					pin = '';
					edit();
				}}><option value="tx">Transaction</option><option value="box">Box</option></select
			>
		</div>
		<div class="id-field">
			<label for="application-id">Transaction or box ID</label><input
				id="application-id"
				bind:value={id}
				oninput={edit}
				maxlength="64"
				spellcheck="false"
				autocomplete="off"
				placeholder="Paste a 64-character ID"
			/>
		</div>
		<button type="submit" disabled={inspection.loading}
			>{inspection.loading ? 'Inspecting…' : 'Inspect activity'}
			<span aria-hidden="true">↗</span></button
		>
		<p class="request-note">
			One indexed detail request starts the inspection. Following a workflow is a separate action.
			Shared links prefill this form without loading it.
		</p>
		{#if pin}<p class="pin">
				Pinned transaction block <code>{pin}</code><button
					type="button"
					onclick={() => {
						pin = '';
						edit();
					}}>Clear block pin</button
				>
			</p>{/if}
	</form>
	{#if formError || inspection.error}<p role="alert" class="issue">
			{formError || inspection.error}
		</p>{/if}
	{#if inspection.evidence}
		<section class="discovery" aria-label="Application discovery results">
			<header>
				<div>
					<p class="eyebrow">Indexed evidence</p>
					<h2>
						{orders.length + deposits.length} supported {orders.length + deposits.length === 1
							? 'request'
							: 'requests'}
					</h2>
				</div>
				<div class="result-links">
					<a href={`/${inspection.evidence.kind}/${inspection.evidence.value.id}`}
						>Open raw details ↗</a
					><a href={share}>Share this inspection ↗</a>
				</div>
			</header>
			{#if inspection.evidence.kind === 'tx'}<p class="request-note">
					Discovery displays up to four requests per application. Additional requests may exist in
					the raw transaction; this is not an exhaustive count.
				</p>{/if}
			{#if !orders.length && !deposits.length}<p class="unknown">
					No supported request was recognized in this evidence. Other contracts, versions and assets
					remain unclassified. A transaction spending a bridge event may need its original deposit
					box.
				</p>{/if}
			{#key inspection.evidence}
				{#each orders as order (order.box.id)}<ApplicationWorkflow {order} />{/each}
				{#each deposits as deposit (deposit.box.id)}<RosenWorkflow {deposit} />{/each}
			{/key}
		</section>
	{/if}
	<section class="coverage" aria-labelledby="coverage-title">
		<header>
			<p class="eyebrow">Know what is understood</p>
			<h2 id="coverage-title">Supported journeys.</h2>
			<p>
				Recognition follows pinned contract versions. A familiar name or a spent box is not enough
				to establish an outcome.
			</p>
		</header>
		<div class="coverage-grid">
			<article>
				<span class="number">01</span><Icon name="txs" size={24} />
				<h3>Spectrum</h3>
				<p>
					Versioned swaps and liquidity orders, with request parameters, pool evidence and observed
					recipient outputs.
				</p>
				<p class="boundary">
					Only recognized scripts and parameter shapes. Unknown spends stay unclassified.
				</p>
				<a
					href="https://github.com/arkadianet/kadia-explorer/blob/master/docs/product/application-workflows.md"
					target="_blank"
					rel="noreferrer">Read decoder coverage ↗</a
				>
			</article>
			<article>
				<span class="number">02</span><Icon name="trace" size={24} />
				<h3>Rosen</h3>
				<p>
					Native ERG deposits under the supported public-launch contract, linked to matching Ergo
					event evidence.
				</p>
				<p class="boundary">
					A reported external payout is separate from independently confirmed destination-chain
					settlement.
				</p>
				<a href="/applications/rosen">Inspect a bridge deposit ↗</a>
			</article>
			<article>
				<span class="number">03</span><Icon name="layers" size={24} />
				<h3>Your investigation</h3>
				<p>
					Follow individual transactions and boxes beyond a recognized application. Choose each step
					and retain the evidence.
				</p>
				<p class="boundary">
					Connections show transaction membership, without assigning ownership or allocating
					payments.
				</p>
				<a href="/investigate">Open investigation workspace ↗</a>
			</article>
		</div>
	</section>
</section>

<style>
	.applications {
		display: grid;
		gap: 2rem;
		min-width: 0;
	}
	.app-intro {
		display: grid;
		grid-template-columns: 1.3fr 1fr;
		gap: 2rem;
		align-items: end;
		padding: 1rem 0 2rem;
		border-bottom: 1px solid var(--hairline);
	}
	.eyebrow {
		display: flex;
		gap: 0.6rem;
		align-items: center;
		text-transform: uppercase;
		letter-spacing: 0.16em;
		font: 11px var(--font-mono);
		color: var(--fg-muted);
	}
	h1 {
		font-size: clamp(2.5rem, 5vw, 5.3rem);
		line-height: 1.02;
		letter-spacing: -0.055em;
		margin: 1rem 0 0;
	}
	h1 em {
		color: var(--accent-ink);
		font-style: normal;
	}
	.intro-copy {
		max-width: 32rem;
		line-height: 1.8;
	}
	a {
		text-underline-offset: 0.3em;
		text-decoration: underline;
	}
	.intro-copy a {
		display: inline-block;
		margin-top: 1rem;
	}
	.inspect-form {
		display: grid;
		grid-template-columns: 10rem 1fr auto;
		gap: 1rem;
		align-items: end;
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		padding: 1.5rem;
		border-radius: var(--radius-card);
	}
	.inspect-form > div {
		min-width: 0;
	}
	label {
		display: block;
		font-size: 0.8rem;
		margin-bottom: 0.5rem;
	}
	input,
	select {
		width: 100%;
		min-width: 0;
		padding: 0.8rem;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--fg);
	}
	input {
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}
	button {
		padding: 0.8rem 1.2rem;
		border-radius: var(--radius-control);
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		font-weight: 600;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.6;
		cursor: wait;
	}
	.request-note {
		grid-column: 1 / -1;
		font-size: 0.8rem;
		line-height: 1.7;
		color: var(--fg-muted);
		margin: 0;
	}
	.pin {
		grid-column: 1 / -1;
		display: flex;
		gap: 0.6rem;
		align-items: center;
		flex-wrap: wrap;
		font-size: 0.8rem;
	}
	code {
		overflow-wrap: anywhere;
	}
	.pin button {
		background: var(--accent-wash);
		color: var(--fg);
	}
	.issue {
		padding: 1rem;
		border-left: 3px solid var(--accent);
		background: var(--accent-wash);
		overflow-wrap: anywhere;
	}
	.discovery {
		min-width: 0;
		display: grid;
		gap: 1.5rem;
	}
	.discovery > header {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		align-items: end;
		justify-content: space-between;
	}
	h2 {
		font-size: clamp(1.5rem, 3vw, 2.6rem);
		letter-spacing: -0.04em;
	}
	.result-links {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		font-size: 0.85rem;
	}
	.unknown {
		padding: 1.5rem;
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
	}
	.coverage > header {
		max-width: 46rem;
	}
	.coverage > header > p:last-child {
		color: var(--fg-muted);
		line-height: 1.7;
		margin: 0.8rem 0 1.8rem;
	}
	.coverage-grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 1rem;
	}
	article {
		min-width: 0;
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		padding: 1.7rem;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
	}
	article h3 {
		font-size: 1.7rem;
		letter-spacing: -0.035em;
		margin: 1.5rem 0 0.7rem;
	}
	article p {
		line-height: 1.7;
		font-size: 0.9rem;
	}
	article .boundary {
		color: var(--fg-muted);
		font-size: 0.78rem;
		padding: 1rem 0;
	}
	article a {
		margin-top: auto;
		font-size: 0.85rem;
	}
	.number {
		position: absolute;
		right: 1.5rem;
		top: 1.5rem;
		color: var(--fg-muted);
		font-family: var(--font-mono);
	}
	:global(html[data-appearance='prism']) .app-intro {
		padding: 2.5rem;
		border: 1px solid var(--hairline);
		border-radius: 2rem;
		background: linear-gradient(130deg, var(--surface-solid), var(--accent-wash));
		box-shadow: 0 22px 60px color-mix(in srgb, var(--accent) 12%, transparent);
	}
	:global(html[data-appearance='prism']) .coverage-grid {
		grid-template-columns: 1.3fr 1fr 1fr;
	}
	:global(html[data-appearance='prism']) article:first-child {
		transform: translateY(-0.5rem);
		box-shadow: 0 14px 38px color-mix(in srgb, var(--accent) 10%, transparent);
	}
	:global(html[data-appearance='atelier']) .app-intro {
		border-top: 4px double var(--fg);
		border-bottom: 4px double var(--fg);
		padding: 2rem 0;
	}
	:global(html[data-appearance='atelier']) h1,
	:global(html[data-appearance='atelier']) h2,
	:global(html[data-appearance='atelier']) article h3 {
		font-family: var(--font-display, Georgia, serif);
		font-weight: 400;
	}
	:global(html[data-appearance='atelier']) .inspect-form {
		border-radius: 0;
		border-width: 0 0 1px;
		background: transparent;
		padding: 1rem 0;
	}
	:global(html[data-appearance='atelier']) .coverage {
		display: grid;
		grid-template-columns: 0.7fr 1.7fr;
		gap: 2rem;
	}
	:global(html[data-appearance='atelier']) .coverage-grid {
		display: flex;
		flex-direction: column;
	}
	:global(html[data-appearance='atelier']) article {
		border-radius: 0;
		border-width: 1px 0 0;
		background: transparent;
		padding-left: 3rem;
	}
	:global(html[data-appearance='atelier']) .number {
		left: 0;
		right: auto;
	}
	:global(html[data-appearance='aurora']) .app-intro {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		border: 0;
		padding: 2rem;
	}
	:global(html[data-appearance='aurora']) .eyebrow {
		justify-content: center;
	}
	:global(html[data-appearance='aurora']) .coverage > header {
		text-align: center;
		margin: auto;
	}
	:global(html[data-appearance='aurora']) .inspect-form {
		border-radius: 2rem;
		box-shadow: 0 20px 55px color-mix(in srgb, var(--accent) 10%, transparent);
	}
	:global(html[data-appearance='aurora']) article {
		border-radius: 2rem;
		background: linear-gradient(160deg, var(--surface-solid), var(--accent-wash));
	}
	@media (max-width: 800px) {
		.app-intro,
		:global(html[data-appearance='atelier']) .coverage {
			grid-template-columns: 1fr;
		}
		.coverage-grid,
		:global(html[data-appearance='prism']) .coverage-grid {
			grid-template-columns: 1fr;
		}
		.inspect-form {
			grid-template-columns: 1fr;
		}
		:global(html[data-appearance='prism']) .app-intro {
			padding: 1.5rem;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		:global(html[data-appearance='prism']) article:first-child {
			transform: none;
		}
	}
</style>
