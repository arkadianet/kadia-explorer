<script lang="ts">
	import { api } from '$lib/api/endpoints';
	import type { BoxDto, TokenInfoDto } from '$lib/api/types';
	import { tokenMedia } from '$lib/token/media';
	import { tokenDisplayName } from '$lib/token/kind';
	import Hash from './Hash.svelte';
	import Panel from './Panel.svelte';
	import TokenMedia from './TokenMedia.svelte';
	let { token }: { token: TokenInfoDto } = $props();
	let box = $state<BoxDto | null>(null);
	let loading = $state(true);
	let failed = $state(false);
	let attempt = $state(0);
	$effect(() => {
		const mint = token;
		void attempt;
		box = null;
		failed = false;
		loading = true;
		const controller = new AbortController();
		void api
			.box(mint.mint_box, (input, init) => fetch(input, { ...init, signal: controller.signal }))
			.then((result) => {
				if (controller.signal.aborted) return;
				if (
					result.id !== mint.mint_box ||
					result.tx_id !== mint.mint_tx ||
					!result.tokens.some((entry) => entry.id === mint.id)
				) {
					failed = true;
					return;
				}
				box = result;
			})
			.catch(() => {
				if (!controller.signal.aborted) failed = true;
			})
			.finally(() => {
				if (!controller.signal.aborted) loading = false;
			});
		return () => controller.abort();
	});
	const media = $derived(box ? tokenMedia(token.kind, box.registers) : null);
</script>

<Panel title="Mint provenance">
	<div class="provenance" class:has-media={!!media}>
		<div class="record">
			<p class="context">
				The token ID comes from the mint transaction’s first input box. Its name, description and
				media are declarations made at mint, not a verified issuer identity.
			</p>
			<dl>
				<div>
					<dt>Mint transaction</dt>
					<dd>
						<Hash value={token.mint_tx} href={`/tx/${token.mint_tx}`} copy={false} head={12} />
					</dd>
				</div>
				<div>
					<dt>Minting output</dt>
					<dd>
						<Hash value={token.mint_box} href={`/box/${token.mint_box}`} copy={false} head={12} />
					</dd>
				</div>
				<div>
					<dt>Mint input / token ID</dt>
					<dd><Hash value={token.id} href={`/box/${token.id}`} copy={false} head={12} /></dd>
				</div>
				<div>
					<dt>Included at height</dt>
					<dd><a href={`/blocks/${token.mint_height}`}>{token.mint_height}</a></dd>
				</div>
				{#if box?.address}<div>
						<dt>Mint output address</dt>
						<dd>
							<Hash value={box.address} href={`/address/${box.address}`} copy={false} head={12} />
						</dd>
					</div>{/if}
			</dl>
			{#if loading}<p class="context" role="status">Reading mint output…</p>
			{:else if failed}<p class="context">
					Mint output metadata is unavailable or does not match this mint. The provenance links
					remain available.
				</p>
				<button type="button" class="btn" onclick={() => attempt++}>Retry mint output</button>{/if}
			<p class="context">
				Supply is indexed minted amount minus indexed burns; it does not estimate liquid or
				circulating supply. Holder counts refer to locking scripts, including contracts, and do not
				count unique people.
			</p>
			{#if token.decimals === null}<p class="context">
					No decimal precision was declared. Amounts are shown as raw integer units.
				</p>{/if}
		</div>
		{#if media}{#key token.id}<TokenMedia
					{media}
					name={tokenDisplayName(token.name, token.id)}
				/>{/key}{/if}
	</div>
</Panel>

<style>
	.provenance {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 28px;
	}
	.provenance.has-media {
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
	}
	.record {
		min-width: 0;
	}
	.context {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.7;
		max-width: 76ch;
	}
	dl {
		margin: 22px 0;
	}
	dl > div {
		display: grid;
		grid-template-columns: minmax(130px, 0.7fr) minmax(0, 1fr);
		gap: 12px;
		padding: 11px 0;
		border-bottom: var(--rule);
		font-size: 12px;
	}
	dt {
		color: var(--fg-muted);
	}
	dd {
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.context + .context {
		margin-top: 10px;
	}
	button {
		margin: 12px 0;
	}
	@media (max-width: 900px) {
		.provenance.has-media {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@media (max-width: 500px) {
		dl > div {
			grid-template-columns: minmax(0, 1fr);
			gap: 6px;
		}
	}
</style>
