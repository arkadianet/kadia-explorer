<script lang="ts">
	import { decodeRegister, REGISTER_KEYS } from '$lib/registers/decode';
	import type { BoxDto } from '$lib/api/types';

	interface Props {
		registers: BoxDto['registers'];
	}

	let { registers }: Props = $props();

	interface RegisterRow {
		key: string;
		raw: string;
		type: string;
		value: string;
	}

	const rows = $derived.by((): RegisterRow[] => {
		const regs = registers;
		if (!regs) return [];
		const out: RegisterRow[] = [];
		for (const key of REGISTER_KEYS) {
			const raw = regs[key];
			if (typeof raw !== 'string' || raw === '') continue;
			const decoded = decodeRegister(raw);
			out.push({
				key,
				raw,
				type: decoded?.type ?? 'raw',
				value: decoded?.value ?? raw
			});
		}
		return out;
	});
</script>

{#if rows.length > 0}
	<table class="regs">
		<tbody>
			{#each rows as reg (reg.key)}
				<tr>
					<th scope="row" class="reg-key">{reg.key}</th>
					<td class="reg-type">{reg.type}</td>
					<td class="mono reg-value" title={reg.raw}>{reg.value}</td>
				</tr>
			{/each}
		</tbody>
	</table>
{/if}

<style>
	.regs {
		width: 100%;
		border-collapse: collapse;
		margin-top: var(--space-1);
	}
	.reg-key {
		color: var(--fg-muted);
		width: 1%;
		padding-right: var(--space-2);
		white-space: nowrap;
		font-weight: normal;
		text-align: left;
	}
	.reg-type {
		color: var(--fg-muted);
		width: 1%;
		padding-right: var(--space-2);
		white-space: nowrap;
		font-size: 11px;
	}
	.reg-value {
		overflow-wrap: anywhere;
	}
</style>
