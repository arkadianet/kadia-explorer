<script lang="ts">
	import { savedAddresses } from '$lib/addresses/saved.svelte';
	import Icon from './Icon.svelte';
	let { address, compact = false }: { address: string; compact?: boolean } = $props();
	const uid = $props.id();
	let dialog: HTMLDialogElement;
	let opener: HTMLButtonElement;
	let input: HTMLInputElement;
	let label = $state('');
	let group = $state('');
	let announcement = $state('');
	const saved = $derived(savedAddresses.items.find((item) => item.address === address));
	const groups = $derived(
		savedAddresses.items
			.map((item) => item.group ?? '')
			.filter((name, index, all) => name && all.indexOf(name) === index)
			.sort((a, b) => a.localeCompare(b))
	);
	function open() {
		label = saved?.label ?? '';
		group = saved?.group ?? '';
		announcement = '';
		dialog.showModal();
		input.focus();
	}
	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (savedAddresses.save(address, label, group)) {
			dialog.close();
			announcement = 'Saved in this browser.';
		}
	}
	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') event.stopPropagation();
		if (event.key !== 'Tab') return;
		const controls = [...dialog.querySelectorAll<HTMLElement>('button, input')];
		if (event.shiftKey && document.activeElement === controls[0]) {
			event.preventDefault();
			controls.at(-1)?.focus();
		} else if (!event.shiftKey && document.activeElement === controls.at(-1)) {
			event.preventDefault();
			controls[0]?.focus();
		}
	}
</script>

<div class="save-address">
	{#if saved?.label && !compact}<p class="local-label">Local label <bdi>{saved.label}</bdi></p>{/if}
	{#if saved?.group && !compact}<p class="local-label">Group <bdi>{saved.group}</bdi></p>{/if}
	<button
		class="save-trigger"
		type="button"
		bind:this={opener}
		onclick={open}
		aria-haspopup="dialog"
		><Icon name="addresses" size={16} />{saved ? 'Edit saved address' : 'Save address'}</button
	>
	{#if !compact}<a href="/saved">Saved addresses</a>{/if}
	{#if announcement}<p role="status">{announcement}</p>{/if}
</div>
<dialog
	bind:this={dialog}
	aria-labelledby={`${uid}-title`}
	aria-describedby={`${uid}-description`}
	onclose={() => opener?.focus()}
	onkeydown={keys}
>
	<header>
		<h2 id={`${uid}-title`}>{saved ? 'Edit saved address' : 'Save address'}</h2>
		<button type="button" onclick={() => dialog.close()}>Close</button>
	</header>
	<p id={`${uid}-description`}>
		Keep an address, optional label and group in this browser. Groups organize your list; labels and
		groups do not establish ownership or verification.
	</p>
	<p class="address mono">{address}</p>
	<form onsubmit={submit}>
		<label for={`${uid}-label`}>Local label <span>(optional, 80 characters maximum)</span></label>
		<input
			id={`${uid}-label`}
			class="control"
			bind:this={input}
			bind:value={label}
			maxlength="80"
			autocomplete="off"
			placeholder="A name you recognise"
		/>
		<label for={`${uid}-group`}>Group <span>(optional, 40 characters maximum)</span></label>
		<input
			id={`${uid}-group`}
			class="control"
			bind:value={group}
			maxlength="40"
			autocomplete="off"
			list={`${uid}-groups`}
			placeholder="Choose or create a local group"
			aria-describedby={`${uid}-group-help`}
		/>
		<datalist id={`${uid}-groups`}
			>{#each groups as name (name)}<option value={name}></option>{/each}</datalist
		>
		<p id={`${uid}-group-help`}>
			Use an existing group name or enter a new one. Clear this field to leave the address
			ungrouped.
		</p>
		{#if savedAddresses.error}<p class="error" role="alert">{savedAddresses.error}</p>{/if}
		<button type="submit" class="btn btn-fill"
			>{saved ? 'Save changes' : 'Save in this browser'}</button
		>
	</form>
</dialog>

<style>
	.save-address {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		font-size: 12px;
	}
	.save-trigger {
		display: flex;
		align-items: center;
		gap: 8px;
		border: var(--rule);
		border-radius: var(--radius-control);
		padding: 9px 14px;
		background: var(--surface-solid);
		color: var(--fg);
		cursor: pointer;
	}
	.local-label {
		display: flex;
		gap: 10px;
		color: var(--fg-muted);
	}
	.local-label bdi {
		color: var(--fg);
		font-weight: 650;
		overflow-wrap: anywhere;
	}
	a {
		color: var(--accent-ink);
		text-decoration: underline;
	}
	dialog {
		width: min(560px, calc(100vw - 28px));
		max-height: calc(100dvh - 36px);
		overflow: auto;
		padding: 24px;
		background: var(--surface-solid);
		color: var(--fg);
		border: var(--rule);
		border-radius: var(--radius-card);
		box-shadow: var(--shadow-lift);
	}
	dialog::backdrop {
		background: #07111c99;
		backdrop-filter: blur(5px);
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		margin-bottom: 16px;
	}
	h2 {
		font: var(--weight-display, 700) 27px var(--font-display, var(--font-sans));
	}
	header button {
		padding: 8px 12px;
		border: var(--rule);
		background: var(--bg);
		color: var(--fg);
		border-radius: 8px;
		cursor: pointer;
	}
	dialog p {
		font-size: 13px;
		color: var(--fg-muted);
		line-height: 1.65;
	}
	.address {
		overflow-wrap: anywhere;
		margin: 18px 0;
		padding: 12px;
		background: var(--surface-hover);
	}
	form {
		display: grid;
		gap: 14px;
	}
	label {
		font-size: 13px;
		font-weight: 600;
	}
	label span {
		display: block;
		font-size: 11px;
		color: var(--fg-muted);
		font-weight: 400;
	}
	.error {
		color: var(--danger-ink);
	}
	form .btn {
		justify-content: center;
		white-space: normal;
		height: auto;
		min-height: 44px;
	}
	:global(:root[data-appearance='atelier']) dialog {
		border-radius: 0;
		border-top: 4px double var(--fg);
	}
	:global(:root[data-appearance='aurora']) dialog {
		padding: 32px;
		border-radius: 24px;
	}
	:global(:root[data-appearance='prism']) dialog {
		border-left: 5px solid var(--accent-ink);
	}
	@media (max-width: 400px) {
		dialog {
			padding: 18px;
		}
		header {
			align-items: start;
		}
		h2 {
			font-size: 23px;
		}
	}
</style>
