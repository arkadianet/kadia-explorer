<script lang="ts">
	import { appearance, type Appearance } from '$lib/theme/theme.svelte';
	import Icon from './Icon.svelte';

	const choices: { id: Appearance; name: string; description: string }[] = [
		{ id: 'original', name: 'Original', description: 'Classic sidebar and compact dashboard.' },
		{ id: 'aurora', name: 'Aurora', description: 'Floating navigation and a luminous canvas.' },
		{ id: 'atelier', name: 'Atelier', description: 'Editorial masthead and a paper ledger.' },
		{ id: 'prism', name: 'Prism', description: 'Full-width workspace and spatial transactions.' }
	];
	let dialog: HTMLDialogElement;
	let opener: HTMLButtonElement;
	let isOpen = $state(false);
	let announcement = $state('');
	const currentName = $derived(choices.find((choice) => choice.id === appearance.current)!.name);

	function show() {
		dialog.showModal();
		isOpen = true;
		dialog.querySelector<HTMLInputElement>('input:checked')?.focus();
	}
	function closed() {
		isOpen = false;
		opener?.focus();
	}
	function select(choice: (typeof choices)[number]) {
		appearance.set(choice.id);
		announcement = `${choice.name} selected.`;
	}
	function keys(event: KeyboardEvent) {
		// Keep the mobile navigation's Escape handler from taking focus out of the modal.
		if (event.key === 'Escape') event.stopPropagation();
		if (event.key !== 'Tab') return;
		// A native radio group has one Tab stop: its selected option. Arrow keys move within it.
		const controls = Array.from(
			dialog.querySelectorAll<HTMLElement>('button:not([disabled]), input[type="radio"]:checked')
		);
		const first = controls[0];
		const last = controls.at(-1);
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last?.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first?.focus();
		}
	}
</script>

<button
	type="button"
	class="appearance-trigger"
	aria-label="Appearance"
	aria-describedby="current-appearance"
	aria-haspopup="dialog"
	aria-expanded={isOpen}
	aria-controls="appearance-dialog"
	bind:this={opener}
	onclick={show}
>
	<Icon name="layers" size={17} /><span class="trigger-label">Style</span>
</button>
<span id="current-appearance" class="visually-hidden">{currentName} selected</span>

<dialog
	id="appearance-dialog"
	class="appearance-dialog"
	bind:this={dialog}
	aria-labelledby="appearance-heading"
	aria-describedby="appearance-description"
	onclose={closed}
	onkeydown={keys}
>
	<header class="appearance-heading">
		<div>
			<h2 id="appearance-heading">Choose appearance</h2>
			<p id="appearance-description">
				Choose a layout and visual style. Every design supports light and dark mode.
			</p>
		</div>
		<button type="button" class="close-appearance" onclick={() => dialog.close()}>Close</button>
	</header>
	<fieldset class="appearance-options">
		<legend class="visually-hidden">Explorer style</legend>
		{#each choices as choice (choice.id)}
			<label class="appearance-choice" class:selected={appearance.current === choice.id}>
				<span class="appearance-preview {choice.id}" aria-hidden="true">
					<span class="preview-rail"><span></span><span></span><span></span></span>
					<span class="preview-main"
						><span class="preview-type">Aa</span><span class="preview-art"></span><span
							class="preview-rule"
						></span></span
					>
				</span>
				<span class="choice-content">
					<span class="choice-heading">
						<input
							type="radio"
							name="explorer-appearance"
							value={choice.id}
							aria-label={choice.name}
							aria-describedby={`appearance-${choice.id}-description`}
							checked={appearance.current === choice.id}
							onchange={() => select(choice)}
						/>
						<span class="choice-name">{choice.name}</span>
					</span>
					<span id={`appearance-${choice.id}-description`} class="choice-description"
						>{choice.description}</span
					>
				</span>
			</label>
		{/each}
	</fieldset>
	<p class="appearance-feedback" role="status" aria-live="polite">
		{announcement || `${currentName} is selected.`}
	</p>
</dialog>

<style>
	.appearance-trigger {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 7px;
		flex: none;
		min-height: 34px;
		padding: 7px 11px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--surface-solid);
		color: var(--fg);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.appearance-trigger:hover {
		border-color: var(--fg-muted);
	}
	.appearance-dialog {
		width: min(680px, calc(100vw - 32px));
		max-height: calc(100dvh - 40px);
		margin: auto;
		padding: 0;
		border: var(--rule);
		border-radius: 18px;
		background: var(--surface-solid);
		color: var(--fg);
		box-shadow: 0 24px 100px #0006;
	}
	.appearance-dialog::backdrop {
		background: #03100db3;
		backdrop-filter: blur(5px);
	}
	.appearance-heading {
		position: sticky;
		top: 0;
		z-index: 1;
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 20px;
		padding: 24px;
		border-bottom: var(--rule);
		background: var(--surface-solid);
	}
	.appearance-heading h2 {
		font-size: 23px;
		line-height: 1.2;
		font-weight: 700;
		letter-spacing: -0.04em;
	}
	.appearance-heading p {
		max-width: 45ch;
		margin-top: 9px;
		color: var(--fg-muted);
		font-size: 13px;
	}
	.close-appearance {
		flex: none;
		padding: 9px 12px;
		border: var(--rule);
		border-radius: var(--radius-control);
		background: var(--bg);
		color: var(--fg);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.appearance-options {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 16px;
		min-width: 0;
		margin: 0;
		padding: 24px;
		border: 0;
	}
	.appearance-choice {
		min-width: 0;
		overflow: hidden;
		border: 1px solid var(--hairline);
		border-radius: 12px;
		background: var(--bg);
		cursor: pointer;
	}
	.appearance-choice.selected {
		border-color: var(--accent-ink);
		box-shadow: 0 0 0 1px var(--accent-ink);
	}
	.choice-content {
		display: block;
		padding: 14px;
	}
	.choice-heading {
		display: flex;
		gap: 9px;
		align-items: center;
	}
	.choice-heading input {
		width: 16px;
		height: 16px;
		flex: none;
		margin: 0;
		accent-color: var(--accent-ink);
	}
	.choice-name {
		font-size: 16px;
		font-weight: 700;
	}
	.choice-description {
		display: block;
		margin-top: 6px;
		color: var(--fg-muted);
		font-size: 12px;
	}
	.appearance-feedback {
		min-height: 18px;
		padding: 0 24px 20px;
		color: var(--fg-muted);
		font-size: 12px;
	}
	.appearance-preview {
		--preview-bg: #eef3ef;
		--preview-ink: #10291e;
		--preview-accent: #b7f25f;
		display: flex;
		height: 114px;
		background: var(--preview-bg);
		color: var(--preview-ink);
		border-bottom: 1px solid var(--hairline);
	}
	.preview-rail {
		display: grid;
		align-content: start;
		gap: 10px;
		width: 29px;
		flex: none;
		padding: 14px 8px;
		background: var(--preview-ink);
	}
	.preview-rail > span {
		height: 3px;
		border-radius: 2px;
		background: var(--preview-accent);
		opacity: 0.65;
	}
	.preview-rail > span:first-child {
		opacity: 1;
		height: 7px;
	}
	.preview-main {
		position: relative;
		flex: 1;
		min-width: 0;
		overflow: hidden;
		padding: 16px 18px;
	}
	.preview-type {
		position: relative;
		z-index: 1;
		display: block;
		font-family: system-ui, sans-serif;
		font-size: 39px;
		font-weight: 700;
		letter-spacing: -0.08em;
		line-height: 1.25;
	}
	.preview-rule {
		display: block;
		margin-top: 8px;
		width: 56%;
		height: 5px;
		border-radius: 4px;
		background: var(--preview-ink);
		opacity: 0.15;
	}
	.preview-art {
		position: absolute;
		right: 22px;
		top: 23px;
		width: 46px;
		height: 46px;
		background: var(--preview-accent);
		border-radius: 12px;
	}
	.aurora {
		--preview-bg: #081710;
		--preview-ink: #dff5e5;
		--preview-accent: #91eeb7;
		background: radial-gradient(ellipse at 90% 65%, #254f3b, #081710 70%);
	}
	.aurora .preview-rail {
		width: 15px;
		padding-inline: 4px;
		background: #183426;
	}
	.aurora .preview-type {
		font-family: Georgia, serif;
		font-style: italic;
		font-weight: 400;
	}
	.aurora .preview-art {
		background: linear-gradient(140deg, #8bcbaa99, #0a1e11);
		border: 1px solid #a2e6ba77;
		box-shadow: 0 12px 20px #73e5a333;
		transform: rotate(-30deg) skew(8deg);
		border-radius: 4px;
	}
	.atelier {
		--preview-bg: #f5efe3;
		--preview-ink: #332b20;
		--preview-accent: #aa4627;
	}
	.atelier .preview-rail {
		background: #ded4c3;
		border-bottom: 2px double #b5a38b;
	}
	.atelier .preview-type {
		font-family: Georgia, serif;
		font-weight: 400;
		letter-spacing: -0.06em;
	}
	.atelier .preview-art {
		background: repeating-linear-gradient(0deg, #aa4627 0 1px, transparent 1px 8px);
		border: 1px solid #aa4627;
		border-radius: 50%;
	}
	.atelier .preview-rule {
		height: 1px;
		border-radius: 0;
		opacity: 0.6;
	}
	.prism {
		--preview-bg: #e8f2fc;
		--preview-ink: #193553;
		--preview-accent: #477fe1;
		background: radial-gradient(ellipse at 88% 70%, #b9d5fc, #edf5fc 73%);
	}
	.prism .preview-rail {
		background: #d5e4f5;
	}
	.atelier,
	.prism {
		flex-direction: column;
	}
	.atelier .preview-rail,
	.prism .preview-rail {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: 16px;
		padding: 4px 12px;
	}
	.atelier .preview-rail > span,
	.prism .preview-rail > span {
		width: 18px;
		height: 3px;
	}
	.prism .preview-type {
		font-weight: 500;
		letter-spacing: -0.09em;
	}
	.prism .preview-art {
		background: linear-gradient(135deg, #f5fcff, #5d8fe3);
		border: 1px solid #fff;
		box-shadow:
			8px 9px 0 #80aaf178,
			0 14px 24px #2466b52b;
		transform: rotate(-30deg) skew(8deg);
		border-radius: 3px;
	}
	@media (max-width: 540px) {
		.appearance-trigger {
			width: 34px;
			padding: 7px;
		}
		.trigger-label {
			display: none;
		}
		.appearance-heading {
			padding: 18px 16px;
			gap: 14px;
		}
		.appearance-heading h2 {
			font-size: 20px;
		}
		.appearance-heading p {
			font-size: 12px;
		}
		.appearance-options {
			padding: 18px 16px;
			gap: 12px;
		}
		.appearance-preview {
			height: 90px;
		}
		.preview-rail {
			width: 18px;
			padding-inline: 5px;
		}
		.preview-main {
			padding: 15px 10px;
		}
		.preview-type {
			font-size: 30px;
		}
		.preview-art {
			width: 29px;
			height: 29px;
			right: 10px;
			top: 31px;
		}
		.choice-content {
			padding: 12px 10px;
		}
		.choice-heading {
			gap: 6px;
		}
		.choice-name {
			font-size: 14px;
		}
		.appearance-feedback {
			padding-inline: 16px;
		}
	}
	@media (pointer: coarse) {
		.appearance-trigger,
		.close-appearance {
			min-height: 44px;
		}
		.appearance-trigger {
			min-width: 44px;
		}
	}
</style>
