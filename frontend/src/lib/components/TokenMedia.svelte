<script lang="ts">
	import { onDestroy } from 'svelte';
	import { fetchMedia, type tokenMedia } from '$lib/token/media';
	let { media, name }: { media: NonNullable<ReturnType<typeof tokenMedia>>; name: string } =
		$props();
	let preview = $state<string | null>(null);
	let loading = $state(false);
	let failure = $state<string | null>(null);
	let verified = $state(false);
	let controller: AbortController | null = null;
	let generation = 0;
	function unload() {
		generation++;
		controller?.abort();
		controller = null;
		if (preview) URL.revokeObjectURL(preview);
		preview = null;
		loading = false;
		verified = false;
	}
	onDestroy(unload);
	async function load() {
		if (!media.source) return;
		unload();
		const request = generation;
		const abort = new AbortController();
		controller = abort;
		loading = true;
		failure = null;
		const timeout = setTimeout(() => abort.abort(), 30_000);
		try {
			const file = await fetchMedia(media.source, abort.signal);
			if (request !== generation) return;
			if (media.hash) {
				if (!globalThis.crypto?.subtle)
					throw new Error('This browser cannot verify the declared hash. Preview withheld.');
				const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
				const actual = Array.from(new Uint8Array(digest), (byte) =>
					byte.toString(16).padStart(2, '0')
				).join('');
				if (actual !== media.hash)
					throw new Error(
						'The downloaded bytes do not match the declared SHA-256. Preview withheld.'
					);
				if (request !== generation) return;
				verified = true;
			}
			preview = URL.createObjectURL(file);
		} catch (error) {
			if (request === generation)
				failure =
					error instanceof Error && error.name !== 'TypeError' && error.name !== 'AbortError'
						? error.message
						: 'The host did not allow a private preview, redirected the request, or timed out. You can open its link directly.';
		} finally {
			clearTimeout(timeout);
			if (request === generation) loading = false;
		}
	}
	function failedPlayback() {
		unload();
		failure = 'This browser could not display the file in its declared media format.';
	}
</script>

<section class="media" aria-label="Token media">
	<div class="media-heading">
		<h3>Declared {media.kind}</h3>
		<span>EIP-4 mint metadata</span>
	</div>
	{#if media.source}
		{#if preview}
			<div class="preview">
				{#if media.kind === 'image'}
					<img src={preview} alt={`${name} — media declared at mint`} onerror={failedPlayback} />
				{:else if media.kind === 'audio'}
					<audio
						src={preview}
						controls
						preload="metadata"
						onerror={failedPlayback}
						aria-label={`${name} — audio declared at mint`}>Audio playback is not supported.</audio
					>
				{:else}
					<!-- svelte-ignore a11y_media_has_caption (Mint metadata provides no caption track; controls and a labelled source link remain available.) -->
					<video
						src={preview}
						controls
						preload="metadata"
						onerror={failedPlayback}
						aria-label={`${name} — video declared at mint`}>Video playback is not supported.</video
					>
				{/if}
			</div>
			<p class="verification">
				{verified
					? 'Downloaded bytes match the SHA-256 recorded at mint. This does not verify the creator or ownership rights.'
					: 'No usable SHA-256 was declared. This preview has not been verified against a mint hash.'}
			</p>
		{:else}
			<div class="placeholder">
				<span aria-hidden="true"
					>{media.kind === 'image' ? '▧' : media.kind === 'audio' ? '♫' : '▷'}</span
				>
				<p>External media stays unloaded until you choose to view it.</p>
			</div>
		{/if}
		<p class="privacy">
			Loading contacts <strong>{media.source.host}</strong>{media.source.gateway
				? ', the IPFS gateway'
				: ''}, which can see your IP address. No cookies or referrer are sent. Previews are limited
			to 32 MiB and do not autoplay.
		</p>
		<div class="actions">
			{#if preview || loading}<button type="button" class="btn" onclick={unload}
					>{loading ? 'Cancel media load' : 'Unload media'}</button
				>
			{:else}<button type="button" class="btn btn-primary" onclick={() => void load()}
					>{failure ? 'Retry external media' : 'Load external media'}</button
				>{/if}
			<a href={media.source.url} target="_blank" rel="noopener noreferrer">Open declared media ↗</a>
		</div>
		{#if loading}<p role="status">Loading external file…</p>{/if}
		{#if failure}<p class="failure" role="status">{failure}</p>{/if}
	{:else}
		<p class="unsupported">{media.reason}</p>
	{/if}
	{#if media.uri}<details><summary>Declared URI</summary><code>{media.uri}</code></details>{/if}
	{#if media.hash}<details>
			<summary>Declared SHA-256</summary><code>{media.hash}</code>
			<p>Only a successfully downloaded preview with a matching hash is marked as checked.</p>
		</details>{/if}
</section>

<style>
	.media {
		min-width: 0;
		padding: 24px;
		border: var(--rule);
		border-radius: var(--radius-card);
		background: var(--surface-hover);
	}
	.media-heading {
		display: flex;
		gap: 12px;
		align-items: baseline;
		justify-content: space-between;
		flex-wrap: wrap;
	}
	h3 {
		font-size: 21px;
		font-family: var(--font-display, var(--font-sans));
		font-weight: var(--weight-display, 600);
	}
	.media-heading > span,
	p {
		color: var(--fg-muted);
		font-size: 12px;
		line-height: 1.7;
	}
	p {
		margin-top: 12px;
	}
	.placeholder {
		display: grid;
		place-items: center;
		min-height: 160px;
		padding: 24px;
		text-align: center;
		border: 1px dashed var(--hairline);
		border-radius: var(--radius-control);
		margin-top: 20px;
	}
	.placeholder > span {
		font-size: 42px;
		color: var(--accent-ink);
	}
	.preview {
		display: grid;
		place-items: center;
		margin-top: 20px;
		min-width: 0;
		overflow: hidden;
	}
	img,
	video {
		display: block;
		max-width: 100%;
		max-height: 520px;
		object-fit: contain;
		border-radius: var(--radius-control);
	}
	audio {
		display: block;
		width: 100%;
		max-width: 100%;
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 16px;
		flex-wrap: wrap;
		margin-top: 18px;
		font-size: 12px;
	}
	button {
		white-space: normal;
	}
	.failure {
		color: var(--danger-ink);
	}
	details {
		margin-top: 18px;
		font-size: 12px;
	}
	summary {
		cursor: pointer;
		color: var(--fg-muted);
	}
	code {
		display: block;
		margin-top: 10px;
		font-size: 11px;
		overflow-wrap: anywhere;
	}
	@media (max-width: 600px) {
		.media {
			padding: 18px 14px;
		}
	}
</style>
