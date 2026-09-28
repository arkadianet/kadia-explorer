<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { status } from '$lib/status/status.svelte';
	import {
		ENDPOINTS,
		endpointById,
		defaults,
		buildRequest,
		readShare,
		sharePath,
		curlCommand,
		executeRequest,
		continuation,
		type PlaygroundResult
	} from '$lib/developer/playground';
	let key = $state('status');
	let values = $state<Record<string, string>>(defaults(ENDPOINTS[0]));
	let result = $state.raw<(PlaygroundResult & { path: string }) | null>(null);
	let loading = $state(false);
	let error = $state('');
	let notice = $state('');
	let controller: AbortController | undefined;
	let generation = 0;
	const endpoint = $derived(endpointById(key));
	const request = $derived.by(() => {
		try {
			return { path: buildRequest(key, values), error: '' };
		} catch (cause) {
			return { path: '', error: cause instanceof Error ? cause.message : 'Invalid request.' };
		}
	});
	const curl = $derived(
		request.path
			? curlCommand(page.url.origin, request.path)
			: 'Fill in the required parameters to generate a request.'
	);
	const next = $derived(
		result?.ok && result.path === request.path && endpoint.strict ? continuation(result.data) : null
	);
	const formatted = $derived.by(() => {
		if (!result) return '';
		try {
			return JSON.stringify(result.data, null, 2);
		} catch {
			return result.text;
		}
	});
	const groups = [...new Set(ENDPOINTS.map((entry) => entry.group))];
	function stop() {
		generation++;
		controller?.abort();
		controller = undefined;
		loading = false;
	}
	function choose(nextKey: string) {
		stop();
		key = nextKey;
		values = defaults(endpointById(key));
		result = null;
		error = '';
		notice = '';
	}
	$effect(() => {
		const params = page.url.searchParams;
		stop();
		result = null;
		try {
			const saved = readShare(params);
			key = saved.key;
			values = saved.values;
			error = '';
		} catch (cause) {
			key = 'status';
			values = defaults(ENDPOINTS[0]);
			error = cause instanceof Error ? cause.message : 'Invalid shared request.';
		}
	});
	onDestroy(stop);
	async function run() {
		if (request.error) {
			error = request.error;
			return;
		}
		stop();
		const own = ++generation;
		const abort = new AbortController();
		controller = abort;
		const path = request.path;
		const selectedKey = key;
		const fields = { ...values };
		let timedOut = false;
		const timer = setTimeout(() => {
			timedOut = true;
			abort.abort();
		}, 15_000);
		loading = true;
		result = null;
		error = '';
		notice = '';
		try {
			const response = await executeRequest(selectedKey, fields, abort.signal);
			if (own === generation) result = { ...response, path };
		} catch (cause) {
			if (own === generation)
				error = timedOut
					? 'The request exceeded 15 seconds. Retry or use a smaller page.'
					: cause instanceof Error
						? cause.message
						: 'Request failed.';
		} finally {
			clearTimeout(timer);
			if (own === generation) {
				loading = false;
				controller = undefined;
			}
		}
	}
	async function copy(text: string, message: string) {
		try {
			await navigator.clipboard.writeText(text);
			notice = message;
		} catch {
			notice = 'Clipboard access is unavailable. Select and copy the visible request instead.';
		}
	}
	function download() {
		if (!result) return;
		const url = URL.createObjectURL(new Blob([result.text], { type: 'application/json' }));
		const link = document.createElement('a');
		link.href = url;
		link.download = 'kadia-api-response.json';
		link.click();
		setTimeout(() => URL.revokeObjectURL(url), 0);
	}
	function nextPage() {
		if (!next) return;
		values = { ...values, ...next };
		void run();
	}
	function firstPage() {
		values = { ...values, cursor: '', snapshot: '' };
		void run();
	}
</script>

<svelte:head
	><title>API playground — Kadia</title><meta
		name="description"
		content="Build read-only Kadia API requests, inspect real responses and learn snapshot-safe pagination."
	/></svelte:head
>

<section class="developer-workspace">
	<header class="intro">
		<div>
			<p class="eyebrow">For the curious. For the builders.</p>
			<h1>Chain data.<br /><em>Within reach.</em></h1>
		</div>
		<div class="intro-copy">
			<p class="lead">API playground</p>
			<p>Build a request, inspect the evidence and take the exact query into your own tools.</p>
			<div class="traits">
				<span>GET only</span><span>Same-origin requests</span><span>No wallet connection</span>
			</div>
		</div>
	</header>
	<div class="workbench">
		<aside class="catalog" aria-label="API endpoints">
			<label class="mobile-select" for="endpoint-select"
				>Endpoint<select
					id="endpoint-select"
					class="control"
					value={key}
					onchange={(event) => choose(event.currentTarget.value)}
					>{#each ENDPOINTS as entry (entry.id)}<option value={entry.id}>{entry.name}</option
						>{/each}</select
				></label
			>
			<div class="desktop-catalog">
				{#each groups as group (group)}<div class="endpoint-group">
						<h2>{group}</h2>
						{#each ENDPOINTS.filter((entry) => entry.group === group) as entry (entry.id)}<button
								class:selected={entry.id === key}
								aria-pressed={entry.id === key}
								onclick={() => choose(entry.id)}
								><span class="endpoint-dot" aria-hidden="true"></span>{entry.name}</button
							>{/each}
					</div>{/each}
			</div>
		</aside>
		<div class="studio">
			<section class="contract" aria-label="Selected endpoint">
				<p class="eyebrow">{endpoint.group} / v1</p>
				<h2>{endpoint.name}</h2>
				<p>{endpoint.description}</p>
				<div class="route"><span>GET</span><code>/v1{endpoint.path}</code></div>
				<p class="contract-note">{endpoint.note}</p>
			</section>
			<div class="request-response">
				<section class="request-pane" aria-label="Request builder">
					<div class="pane-heading">
						<span class="step">01</span>
						<h2>Build your request</h2>
					</div>
					<form
						onsubmit={(event) => {
							event.preventDefault();
							void run();
						}}
					>
						{#each endpoint.fields as field (`${key}-${field.name}`)}<div class="field">
								<label for={`param-${field.name}`}
									>{field.label}{#if field.required}<span aria-hidden="true"> *</span>{/if}</label
								>{#if field.options}<select
										class="control"
										id={`param-${field.name}`}
										bind:value={values[field.name]}
										>{#each field.options as option (option)}<option value={option}>{option}</option
											>{/each}</select
									>{:else}<input
										class="control"
										id={`param-${field.name}`}
										bind:value={values[field.name]}
										required={field.required}
										maxlength={field.max ?? 4096}
										autocomplete="off"
										spellcheck="false"
										inputmode={field.kind === 'height' || field.kind === 'limit'
											? 'numeric'
											: 'text'}
									/>{/if}{#if field.kind === 'block' && status.current?.indexed !== null && status.current?.indexed !== undefined}<button
										class="text-action"
										type="button"
										onclick={() => {
											values[field.name] = String(status.current?.indexed);
										}}>Use indexed tip</button
									>{/if}
							</div>{/each}
						{#if !endpoint.fields.length}<p class="quiet">
								No parameters needed. Run this request to read the current indexed observation.
							</p>{/if}
						<p class="privacy">
							Requests run only when you choose. A shared link contains the entered IDs, addresses
							and filters; it does not execute them automatically.
						</p>
						<div class="actions">
							<button class="primary" type="submit" disabled={loading || !!request.error}
								>{loading ? 'Reading…' : 'Run request'} <span aria-hidden="true">↗</span></button
							>{#if loading}<button
									type="button"
									onclick={() => {
										stop();
										notice = 'Request cancelled.';
									}}>Cancel request</button
								>{/if}
						</div>
						{#if request.error}<p class="validation">{request.error}</p>{/if}
					</form>
				</section>
				<section class="response-pane" aria-label="API response" aria-busy={loading}>
					<div class="pane-heading">
						<span class="step">02</span>
						<h2>Inspect the response</h2>
					</div>
					{#if error}<p class="error" role="alert">{error}</p>{/if}
					{#if result}<div class="response-metrics" role="status">
							<strong class:bad={!result.ok}>HTTP {result.status}</strong><span
								>{result.elapsed} ms in this browser</span
							><span>{result.bytes.toLocaleString('en-US')} bytes</span>
						</div>
						<code class="sent-path">GET {result.path}</code>{#if result.path !== request.path}<p
								class="quiet"
							>
								This response belongs to the request above. Run again to apply your edited
								parameters.
							</p>{/if}
						<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable JSON needs keyboard focus.) -->
						<pre class="json" tabindex="0" aria-label="JSON response">{formatted.slice(
								0,
								200_000
							)}</pre>
						{#if formatted.length > 200_000}<p class="quiet">
								Display limited to 200,000 characters. Download preserves the complete received
								response.
							</p>{/if}
						<div class="actions">
							<button onclick={download}>Download JSON</button>{#if next}<button
									class="primary"
									onclick={nextPage}>Next page</button
								>{/if}{#if endpoint.strict && (values.cursor || values.snapshot)}<button
									onclick={firstPage}>Restart first page</button
								>{/if}
						</div>{:else}<div class="response-empty">
							<span class="brackets" aria-hidden="true">{`{ }`}</span>
							<h3>{loading ? 'Reading the index…' : 'A request away.'}</h3>
							<p>
								{loading
									? 'You can cancel this read at any time.'
									: 'The response will appear here, including HTTP errors and exact amount strings.'}
							</p>
						</div>{/if}
				</section>
			</div>
			<section class="takeaway" aria-label="Request examples">
				<div class="pane-heading">
					<span class="step">03</span>
					<h2>Take it with you</h2>
				</div>
				<p>POSIX shell example. The host matches this explorer.</p>
				<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable code needs keyboard focus.) -->
				<pre tabindex="0" aria-label="cURL example">{curl}</pre>
				<div class="actions">
					<button disabled={!!request.error} onclick={() => void copy(curl, 'cURL command copied.')}
						>Copy cURL</button
					><button
						disabled={!!request.error}
						onclick={() =>
							void copy(
								new URL(sharePath(key, values), page.url.origin).href,
								'Request link copied. It includes the entered parameters.'
							)}>Copy request link</button
					>
				</div>
				<p role="status">{notice}</p>
			</section>
		</div>
	</div>
	<section class="field-guide" aria-label="API field guide">
		<div>
			<p class="eyebrow">Read the whole contract</p>
			<h2>Small details.<br />Correct answers.</h2>
		</div>
		<div>
			<h3>Keep amounts exact</h3>
			<p>
				ERG amounts use nanoERG: 1 ERG = 1,000,000,000 nanoERG. Token decimals may be unknown. Keep
				raw integer strings intact.
			</p>
		</div>
		<div>
			<h3>Keep pages together</h3>
			<p>
				Continue strict lists with both cursor and snapshot. A 409 requires a fresh walk. A null
				cursor ends membership paging; upcoming rent has its own <code>complete</code> flag.
			</p>
		</div>
		<div>
			<h3>Keep uncertainty visible</h3>
			<p>
				400: invalid request. 404: not found in this view. 422: resource budget. 429: rate limit.
				503: busy, preparing or unavailable—inspect the response code before retrying. Partial
				coverage never proves zero activity.
			</p>
		</div>
	</section>
</section>

<style>
	.developer-workspace {
		min-width: 0;
	}
	.intro {
		display: grid;
		grid-template-columns: 1.2fr 1fr;
		gap: 30px;
		align-items: end;
		padding: 26px 0 34px;
		border-bottom: var(--rule);
		margin-bottom: 28px;
	}
	.eyebrow {
		color: var(--accent-ink);
		font: 10px var(--font-mono);
		letter-spacing: 0.13em;
		text-transform: uppercase;
	}
	h1 {
		font: var(--weight-display, 750) clamp(38px, 5vw, 68px)/1 var(--font-display, var(--font-sans));
		letter-spacing: -0.05em;
		margin-top: 18px;
	}
	h1 em {
		color: var(--accent-ink);
		font-style: normal;
	}
	p {
		line-height: 1.7;
	}
	.intro-copy {
		max-width: 48ch;
		color: var(--fg-muted);
		font-size: 14px;
	}
	.intro-copy .lead {
		color: var(--fg);
		font: 600 23px var(--font-display, var(--font-sans));
		margin-bottom: 14px;
	}
	.traits {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 24px;
		font: 10px var(--font-mono);
	}
	.traits span {
		border: var(--rule);
		padding: 7px 10px;
		border-radius: var(--radius-pill);
	}
	.workbench {
		display: grid;
		grid-template-columns: 185px minmax(0, 1fr);
		gap: 24px;
	}
	.catalog,
	.studio,
	.request-response,
	.response-pane,
	.request-pane,
	.contract,
	.takeaway {
		min-width: 0;
	}
	.endpoint-group {
		margin-bottom: 24px;
	}
	.endpoint-group h2 {
		color: var(--fg-muted);
		font: 10px var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		margin: 4px 0 8px;
	}
	.endpoint-group button {
		border: 0;
		display: flex;
		align-items: center;
		width: 100%;
		text-align: left;
		gap: 8px;
		padding: 10px 8px;
		background: transparent;
		border-radius: var(--radius-control);
		font-size: 12px;
		color: var(--fg-muted);
		cursor: pointer;
	}
	.endpoint-group button.selected {
		background: var(--surface-hover);
		color: var(--accent-ink);
		font-weight: 650;
	}
	.endpoint-dot {
		height: 4px;
		width: 4px;
		background: currentColor;
		border-radius: 50%;
		flex: none;
	}
	.studio {
		display: grid;
		gap: 18px;
		align-content: start;
	}
	.contract,
	.request-pane,
	.response-pane,
	.takeaway {
		padding: 24px;
		background: var(--surface-solid);
		border: var(--rule);
		border-radius: var(--radius-card);
	}
	.contract h2 {
		margin: 12px 0;
		font: 600 27px var(--font-display, var(--font-sans));
	}
	.contract > p:not(.eyebrow),
	.quiet,
	.takeaway > p {
		color: var(--fg-muted);
		font-size: 12px;
	}
	.route {
		display: flex;
		gap: 12px;
		align-items: start;
		margin: 20px 0;
		font: 12px var(--font-mono);
	}
	.route > span {
		color: var(--accent-ink);
		font-weight: 700;
	}
	.route code {
		overflow-wrap: anywhere;
	}
	.contract-note {
		border-left: 2px solid var(--accent-ink);
		padding-left: 14px;
	}
	.request-response {
		display: grid;
		gap: 18px;
	}
	.pane-heading {
		display: flex;
		gap: 12px;
		align-items: center;
		margin-bottom: 20px;
	}
	.pane-heading h2 {
		font: 600 16px var(--font-display, var(--font-sans));
	}
	.step {
		font: 11px var(--font-mono);
		color: var(--accent-ink);
	}
	.field {
		display: grid;
		gap: 7px;
		margin-bottom: 16px;
	}
	.field label,
	.mobile-select {
		font-size: 12px;
		font-weight: 600;
	}
	.field input,
	.field select {
		width: 100%;
		min-width: 0;
	}
	.privacy {
		color: var(--fg-muted);
		font-size: 11px;
		margin: 18px 0;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		align-items: center;
	}
	.actions button {
		border: var(--rule);
		border-radius: var(--radius-control);
		padding: 10px 14px;
		color: var(--fg);
		background: var(--surface-hover);
		font: 600 12px var(--font-sans);
		cursor: pointer;
	}
	.actions .primary {
		background: var(--btn-fill);
		color: var(--btn-fill-fg);
		border-color: transparent;
	}
	.actions button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.text-action {
		border: 0;
		background: none;
		color: var(--accent-ink);
		font-size: 11px;
		text-align: left;
		cursor: pointer;
		padding: 4px 0;
	}
	.validation,
	.error {
		color: var(--danger-ink);
		font-size: 12px;
		margin-top: 16px;
		overflow-wrap: anywhere;
	}
	.response-empty {
		display: grid;
		justify-items: center;
		align-content: center;
		text-align: center;
		min-height: 260px;
		color: var(--fg-muted);
		gap: 16px;
	}
	.brackets {
		font: 64px var(--font-mono);
		color: var(--accent-ink);
		opacity: 0.6;
	}
	.response-empty h3 {
		font: 500 23px var(--font-display, var(--font-sans));
		color: var(--fg);
	}
	.response-empty p {
		max-width: 35ch;
		font-size: 12px;
	}
	.response-metrics {
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		font: 10px var(--font-mono);
		color: var(--fg-muted);
		margin-bottom: 14px;
	}
	.response-metrics strong {
		color: var(--accent-ink);
	}
	.response-metrics strong.bad {
		color: var(--danger-ink);
	}
	.sent-path {
		display: block;
		font-size: 10px;
		overflow-wrap: anywhere;
		color: var(--fg-muted);
		margin-bottom: 12px;
	}
	pre {
		font: 11px/1.7 var(--font-mono);
		overflow: auto;
		max-width: 100%;
		padding: 18px;
		border-radius: var(--radius-control);
		background: #101d29;
		color: #d4e6f2;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		tab-size: 2;
	}
	.json {
		max-height: 520px;
		margin-bottom: 16px;
		white-space: pre;
		overflow-wrap: normal;
	}
	.takeaway pre {
		margin: 14px 0;
	}
	.takeaway [role='status'] {
		margin-top: 10px;
	}
	.field-guide {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 28px;
		padding: 36px 0;
		border-top: var(--rule);
		margin-top: 36px;
	}
	.field-guide h2 {
		font: 500 27px/1.2 var(--font-display, var(--font-sans));
		margin-top: 15px;
	}
	.field-guide h3 {
		font-size: 13px;
		margin-bottom: 12px;
	}
	.field-guide p:not(.eyebrow) {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.mobile-select {
		display: none;
	}
	:global(:root[data-appearance='prism']) .intro {
		padding: 34px;
		background: var(--hero-bg);
		color: var(--hero-fg);
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-lift);
	}
	:global(:root[data-appearance='prism']) .intro-copy,
	:global(:root[data-appearance='prism']) .intro .eyebrow {
		color: var(--hero-muted);
	}
	:global(:root[data-appearance='prism']) .intro-copy .lead,
	:global(:root[data-appearance='prism']) h1 em {
		color: var(--hero-fg);
	}
	:global(:root[data-appearance='prism']) .response-pane {
		box-shadow: var(--shadow-lift);
		border-top: 3px solid var(--accent-ink);
	}
	:global(:root[data-appearance='atelier']) .workbench {
		display: block;
	}
	:global(:root[data-appearance='atelier']) .desktop-catalog {
		display: flex;
		gap: 24px;
		flex-wrap: wrap;
		border-bottom: 3px double var(--fg);
		margin-bottom: 24px;
	}
	:global(:root[data-appearance='atelier']) .endpoint-group {
		flex: 1;
		min-width: 130px;
	}
	:global(:root[data-appearance='atelier']) .contract,
	:global(:root[data-appearance='atelier']) .request-pane,
	:global(:root[data-appearance='atelier']) .response-pane,
	:global(:root[data-appearance='atelier']) .takeaway {
		border: 0;
		border-bottom: var(--rule);
		border-radius: 0;
		background: none;
		padding: 24px 0;
	}
	:global(:root[data-appearance='atelier']) h1 {
		font-size: clamp(46px, 6vw, 82px);
	}
	:global(:root[data-appearance='aurora']) .intro {
		text-align: center;
		display: grid;
		grid-template-columns: 1fr;
		justify-items: center;
		border: 0;
		padding: 30px;
	}
	:global(:root[data-appearance='aurora']) .traits {
		justify-content: center;
	}
	:global(:root[data-appearance='aurora']) .contract {
		border-radius: 26px;
		background: var(--surface-hover);
	}
	:global(:root[data-appearance='aurora']) .catalog {
		border: var(--rule);
		border-radius: 22px;
		padding: 18px 10px;
		align-self: start;
		background: var(--surface-solid);
	}
	@media (min-width: 1200px) {
		.request-response {
			grid-template-columns: minmax(220px, 0.8fr) minmax(0, 1.2fr);
			align-items: start;
		}
	}
	@media (max-width: 900px) {
		.field-guide {
			grid-template-columns: 1fr 1fr;
		}
		.intro {
			grid-template-columns: 1fr;
		}
	}
	@media (max-width: 650px) {
		.workbench {
			grid-template-columns: 1fr;
		}
		.desktop-catalog,
		:global(:root[data-appearance='atelier']) .desktop-catalog {
			display: none;
		}
		.mobile-select {
			display: grid;
			gap: 10px;
		}
		.mobile-select select {
			width: 100%;
			min-width: 0;
		}
		.intro,
		:global(:root[data-appearance='prism']) .intro,
		:global(:root[data-appearance='aurora']) .intro {
			padding: 22px 16px;
		}
		.contract,
		.request-pane,
		.response-pane,
		.takeaway {
			padding: 18px 14px;
		}
		.field-guide {
			grid-template-columns: 1fr;
			gap: 22px;
		}
	}
</style>
