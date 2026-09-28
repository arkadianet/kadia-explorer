<script lang="ts">
	import { untrack } from 'svelte';
	import {
		createRosenWorkflow,
		emptyRosenState,
		rosenApi,
		ROSEN_TARGETS,
		type RosenDeposit
	} from '$lib/apps/rosen';
	import { ROSEN_RELEASE } from '$lib/apps/rosen-contracts';
	import { formatErg } from '$lib/format/amount';
	let { deposit }: { deposit: RosenDeposit } = $props();
	let workflowState = $state(emptyRosenState());
	let controller: ReturnType<typeof createRosenWorkflow> | undefined;
	const key = $derived(
		JSON.stringify([
			deposit.box.id,
			deposit.box.tx_id,
			deposit.box.ergo_tree,
			deposit.box.registers,
			deposit.box.value,
			deposit.box.creation_height
		])
	);
	$effect(() => {
		void key;
		const current = untrack(() => deposit);
		workflowState = emptyRosenState();
		const workflow = createRosenWorkflow({
			deposit: current,
			...rosenApi,
			onChange: (next) => {
				workflowState = next;
			}
		});
		controller = workflow;
		return () => workflow.stop();
	});
	const stage = $derived(
		workflowState.error
			? 'Inspection unavailable'
			: !workflowState.checked
				? 'Ready to inspect'
				: workflowState.lookupTruncated || workflowState.events.length > 1
					? 'Multiple or incomplete event records'
					: workflowState.spend?.kind === 'reward'
						? 'Reward-distribution stage observed'
						: workflowState.spend?.kind === 'cleanup'
							? 'Cleanup-shaped spend observed'
							: workflowState.spend
								? 'Event spent · outcome unrecognized'
								: workflowState.events.length
									? 'Matching event observed'
									: 'No matching event found'
	);
	const paymentLink = $derived(
		workflowState.spend?.paymentId
			? (deposit.toChain === 'cardano'
					? 'https://cardanoscan.io/transaction/'
					: deposit.toChain === 'ethereum'
						? 'https://etherscan.io/tx/'
						: 'https://bscscan.com/tx/') + workflowState.spend.paymentId
			: null
	);
	function inspect() {
		untrack(() => {
			void controller?.inspect();
		});
	}
</script>

<section
	class="rosen-workflow"
	aria-label="Rosen bridge workflow"
	aria-busy={workflowState.loading}
>
	<header>
		<p class="eyebrow">APPLICATION / ROSEN BRIDGE</p>
		<h2>Across chains. With evidence.</h2>
		<p>Follow a supported native ERG deposit from Ergo to {deposit.toChain}.</p>
		<p class="stage" role="status">
			{workflowState.loading ? 'Inspecting indexed evidence…' : stage}
		</p>
	</header>
	<div class="journey">
		<section class="deposit" aria-label="Bridge deposit request">
			<p class="eyebrow">01 / DEPOSIT REQUEST</p>
			<p class="amount">{formatErg(deposit.amount)} <span>ERG</span></p>
			<p class="muted">
				Value placed in the supported Rosen lock box. Destination validity and acceptance are not
				established by this metadata.
			</p>
			<dl>
				<div>
					<dt>Destination chain</dt>
					<dd>{deposit.toChain}</dd>
				</div>
				<div>
					<dt>Declared recipient</dt>
					<dd class="mono">{deposit.toAddress}</dd>
				</div>
				<div>
					<dt>Declared source address</dt>
					<dd class="mono">{deposit.fromAddress}</dd>
				</div>
				<div>
					<dt>Declared bridge fee</dt>
					<dd>{formatErg(deposit.bridgeFee)} ERG units</dd>
				</div>
				<div>
					<dt>Declared network fee</dt>
					<dd>{formatErg(deposit.networkFee)} ERG units</dd>
				</div>
			</dl>
			<p class="muted">
				Fees use the pinned ERG/rsERG unit scale. These are requested bridge fees, separate from the
				Ergo transaction fee; they do not promise a final payout.
			</p>
			<div class="links">
				<a href={'/box/' + deposit.box.id}>Deposit box</a><a href={'/tx/' + deposit.box.tx_id}
					>Creating transaction</a
				>
			</div>
		</section>
		<section class="observations" aria-label="Bridge event observations">
			<p class="eyebrow">02 / ERGO EVENT EVIDENCE</p>
			{#if workflowState.error}
				<div role="alert">
					<h3>Evidence unavailable</h3>
					<p>{workflowState.error}</p>
					<p>No previous bridge outcome is retained. Retry explicitly.</p>
				</div>
			{:else if workflowState.checked && workflowState.source}
				<p>
					Deposit creation is indexed in <a href={'/blocks/' + workflowState.source.block_id}
						>block {workflowState.source.height.toLocaleString('en-US')}</a
					>.
				</p>
				{#if workflowState.lookupAnchor}<p class="muted">
						Event records observed at <a href={'/blocks/' + workflowState.lookupAnchor.block_id}
							>indexed block {workflowState.lookupAnchor.height.toLocaleString('en-US')}</a
						>. This is a loaded snapshot.
					</p>{/if}
				{#if workflowState.events.length === 0}
					<h3>No matching event in this indexed result</h3>
					<p>
						This is not a rejection, timeout, or proof that watchers have not reported it. Other
						Rosen versions and incomplete retained history are outside this lookup.
					</p>
				{:else}
					<ul class="events">
						{#each workflowState.events as event (event.box.id)}<li>
								<a href={'/box/' + event.box.id}>Event {event.box.id.slice(0, 10)}…</a><span
									>{event.watchers.toLocaleString('en-US')} declared watcher commitments</span
								><a href={'/tx/' + event.box.tx_id}>Event creation</a>
							</li>{/each}
					</ul>
					<p class="muted">
						The exact event data matches this deposit and its source block. Declared commitment
						counts are not an independent quorum or signature verification.
					</p>
				{/if}
				{#if workflowState.lookupTruncated || workflowState.events.length > 1}<p class="notice">
						This lookup is limited to 10 records. Multiple events or a continuation prevent a single
						outcome interpretation. Each event remains inspectable.
					</p>{/if}
				{#if workflowState.rejectedCandidates}<p class="notice">
						{workflowState.rejectedCandidates} matching-register records did not meet the supported event
						contract and token checks.
					</p>{/if}
				{#if workflowState.spend}
					<h3>
						{workflowState.spend.kind === 'reward'
							? 'Reward-distribution-shaped spend'
							: workflowState.spend.kind === 'cleanup'
								? 'Cleanup-shaped event spend'
								: 'Unrecognized event spend'}
					</h3>
					<p>
						{workflowState.spend.kind === 'reward'
							? 'The indexed spend uses the supported permit output and bridge lock input. This is Ergo-side evidence; it does not independently confirm the destination payment.'
							: workflowState.spend.kind === 'cleanup'
								? 'The event was consumed into the published fraud/cleanup contract shape. This does not independently establish why the event was cleaned up or whether another event paid.'
								: 'A spend alone does not establish payment, failure, or cancellation.'}
					</p>
					<a href={'/tx/' + workflowState.spend.tx.id}>Event spending transaction</a>
				{/if}
			{:else}<h3>Inspect the bridge record.</h3>
				<p>
					Load the deposit’s creating transaction, an exact event-register lookup and, for one
					complete matching event, its spend. Up to four bounded requests run only when you choose.
				</p>{/if}
			<button type="button" onclick={inspect} disabled={workflowState.loading}
				>{workflowState.loading
					? 'Inspecting Rosen workflow…'
					: workflowState.checked || workflowState.error
						? 'Refresh Rosen workflow'
						: 'Inspect Rosen workflow'}</button
			>
		</section>
		<section class="external" aria-label="Destination-chain evidence">
			<p class="eyebrow">03 / DESTINATION CHAIN</p>
			<h3>External confirmation not checked</h3>
			<p>
				Kadia makes no requests to Rosen or destination-chain services. Bridge guards and the
				destination network are separate trust boundaries.
			</p>
			{#if paymentLink && workflowState.spend?.paymentId}<p>Recorded payout transaction ID:</p>
				<p class="mono payment-id">{workflowState.spend.paymentId}</p>
				<a href={paymentLink} target="_blank" rel="noopener noreferrer"
					>Open external {deposit.toChain} transaction ↗</a
				>
				<p class="muted">
					Opening this link contacts a third-party explorer. The reference was recorded on Ergo;
					delivery and finality have not been verified here.
				</p>{:else}<p class="muted">
					No supported destination transaction reference has been loaded. No completion claim is
					made.
				</p>{/if}
		</section>
	</div>
	<details>
		<summary>Supported contracts, assets and limits</summary>
		<p>
			Decoder v1 supports the exact Rosen public-launch 7.1.1 Ergo lock and event scripts, native
			ERG with no other tokens in the deposit, and the pinned Cardano, Ethereum and Binance rsERG
			mappings. Other assets and versions remain unsupported. Source and recipient strings are
			declarations, not ownership evidence.
		</p>
		<p>
			The event lookup is a strict snapshot; transaction reads are separate observations. Deposit
			inclusion is rechecked before publication. This is not consensus replay or live bridge
			monitoring. Refresh failures clear the result.
		</p>
		<p class="mono">Pinned target asset: {ROSEN_TARGETS[deposit.toChain]}</p>
		<a href={ROSEN_RELEASE} target="_blank" rel="noopener noreferrer"
			>Published Rosen contract release ↗</a
		>
	</details>
</section>

<style>
	.rosen-workflow {
		margin-block: 28px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-card);
		background: var(--surface-solid);
		min-width: 0;
		overflow-wrap: anywhere;
	}
	header {
		padding: 28px;
		border-bottom: 1px solid var(--hairline);
	}
	h2 {
		font-size: clamp(26px, 3vw, 40px);
		letter-spacing: -0.045em;
		line-height: 1.12;
		margin-block: 12px;
	}
	h3 {
		font-size: 17px;
		margin-block: 12px 8px;
	}
	p {
		font-size: 12px;
		line-height: 1.75;
		margin-block: 8px;
	}
	.eyebrow {
		font: 10px var(--font-mono);
		letter-spacing: 0.12em;
		color: var(--fg-muted);
	}
	.stage {
		display: inline-block;
		padding: 7px 12px;
		border: 1px solid var(--hairline);
		border-radius: 99px;
	}
	.journey {
		display: grid;
		grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.2fr);
	}
	.journey > section {
		padding: 26px;
		min-width: 0;
	}
	.deposit {
		border-right: 1px solid var(--hairline);
	}
	.external {
		grid-column: 1/-1;
		border-top: 1px solid var(--hairline);
		background: var(--accent-wash);
	}
	.amount {
		font-size: clamp(28px, 4vw, 42px);
		font-weight: 600;
		letter-spacing: -0.04em;
		line-height: 1.1;
		margin-block: 22px;
	}
	.amount span {
		font-size: 13px;
		letter-spacing: 0;
		color: var(--fg-muted);
	}
	.muted {
		color: var(--fg-muted);
		font-size: 11px;
	}
	dl {
		margin-block: 20px;
		display: grid;
		gap: 14px;
	}
	dl > div {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
		gap: 14px;
	}
	dt {
		color: var(--fg-muted);
		font-size: 11px;
	}
	dd {
		margin: 0;
		font-size: 12px;
		min-width: 0;
	}
	.mono {
		font: 11px/1.7 var(--font-mono);
	}
	a {
		color: var(--accent-ink);
		text-decoration: underline;
		text-underline-offset: 3px;
		font-size: 12px;
	}
	.links {
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
	}
	.events {
		padding: 0;
		list-style: none;
		display: grid;
		gap: 12px;
		margin-block: 18px;
	}
	.events li {
		display: grid;
		gap: 6px;
		padding: 12px;
		border: 1px solid var(--hairline);
		border-radius: var(--radius-control);
		font-size: 11px;
	}
	.notice {
		padding: 12px;
		border-left: 3px solid var(--accent-ink);
	}
	button {
		display: block;
		margin-top: 20px;
		padding: 11px 16px;
		border: 1px solid var(--btn-fill);
		border-radius: var(--radius-control);
		color: var(--btn-fill-fg);
		background: var(--btn-fill);
		cursor: pointer;
		font-size: 12px;
		max-width: 100%;
	}
	button:disabled {
		opacity: 0.65;
		cursor: wait;
	}
	details {
		padding: 18px 26px;
		border-top: 1px solid var(--hairline);
		font-size: 11px;
	}
	summary {
		cursor: pointer;
		color: var(--fg-muted);
	}
	:global([data-appearance='prism']) .rosen-workflow {
		background: linear-gradient(140deg, var(--surface-solid), var(--accent-wash));
		border-radius: 24px;
		box-shadow: 0 18px 40px color-mix(in srgb, var(--accent-ink) 10%, transparent);
	}
	:global([data-appearance='prism']) .deposit {
		margin: 18px;
		background: var(--surface-solid);
		border: 1px solid var(--hairline);
		border-radius: 18px;
	}
	:global([data-appearance='atelier']) .rosen-workflow {
		background: transparent;
		border: 0;
		border-top: 3px solid var(--fg);
		border-radius: 0;
	}
	:global([data-appearance='atelier']) .journey {
		grid-template-columns: minmax(0, 0.7fr) minmax(0, 1.3fr);
	}
	:global([data-appearance='atelier']) .deposit dl > div {
		grid-template-columns: 1fr;
		gap: 4px;
	}
	:global([data-appearance='atelier']) .external {
		background: transparent;
		border-block: 3px double var(--hairline);
	}
	:global([data-appearance='aurora']) .rosen-workflow {
		border-radius: 30px;
		padding: 20px;
		background: var(--accent-wash);
	}
	:global([data-appearance='aurora']) header {
		text-align: center;
		border: 0;
	}
	:global([data-appearance='aurora']) .journey {
		gap: 18px;
	}
	:global([data-appearance='aurora']) .journey > section {
		border: 1px solid var(--hairline);
		border-radius: 20px;
		background: var(--surface-solid);
	}
	@media (max-width: 720px) {
		.journey,
		:global([data-appearance='atelier']) .journey {
			grid-template-columns: 1fr;
		}
		header,
		.journey > section,
		details {
			padding: 18px;
		}
		.deposit {
			border-right: 0;
			border-bottom: 1px solid var(--hairline);
		}
		:global([data-appearance='aurora']) .rosen-workflow {
			padding: 10px;
		}
	}
</style>
