<script lang="ts">
	import { absTime, relTime } from '$lib/format/time';

	interface Props {
		ms: number;
	}

	let { ms }: Props = $props();

	let now = $state(Date.now());

	$effect(() => {
		const id = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(id);
	});
</script>

<span title={absTime(ms)}>{relTime(ms, now)}</span>
