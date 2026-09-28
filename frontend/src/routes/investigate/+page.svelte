<script lang="ts">
	import { page } from '$app/state';
	import InvestigationGraph from '$lib/components/InvestigationGraph.svelte';
	import { decodePlan, seedRef, type Plan } from '$lib/investigation/graph';
	const selection = $derived.by((): { plan: Plan; error: string | null } => {
		try {
			const shared = page.url.searchParams.get('plan');
			if (shared !== null) return { plan: decodePlan(shared), error: null };
			const id = page.url.searchParams.get('id');
			const kind = page.url.searchParams.get('kind');
			const seed = seedRef(kind, id);
			return {
				plan: { nodes: seed ? [seed] : [], edges: [] },
				error:
					(id !== null || kind !== null) && !seed
						? 'The seed link needs kind=tx or kind=box and a 64-character hexadecimal ID.'
						: null
			};
		} catch (error) {
			return {
				plan: { nodes: [], edges: [] },
				error: error instanceof Error ? error.message : 'The investigation link is invalid.'
			};
		}
	});
</script>

<svelte:head
	><title>Investigate — Kadia</title><meta
		name="description"
		content="Follow indexed transaction and box references in an explicit, bounded investigation."
	/></svelte:head
>
<InvestigationGraph initial={selection.plan} initialError={selection.error} />
