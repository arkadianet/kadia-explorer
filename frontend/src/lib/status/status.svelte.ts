import { api } from '$lib/api/endpoints';
import type { StatusDto } from '$lib/api/types';

const POLL_MS = 5_000;

let current = $state<StatusDto | null>(null);
let error = $state<unknown>(null);
let timer: ReturnType<typeof setInterval> | undefined;
let visibilityHandler: (() => void) | undefined;

async function poll(): Promise<void> {
	try {
		current = await api.status();
		error = null;
	} catch (e) {
		error = e;
	}
}

function startPolling() {
	if (timer !== undefined) return;
	void poll();
	timer = setInterval(() => void poll(), POLL_MS);
}

function stopPolling() {
	clearInterval(timer);
	timer = undefined;
}

export const status = {
	get current() {
		return current;
	},
	get error() {
		return error;
	},
	/** Starts polling immediately, then every 5 s while the tab is visible; pauses while
	 * hidden and resumes (with an immediate poll) when it becomes visible again. Safe to
	 * call more than once — the layout calls it once on mount. */
	start() {
		if (visibilityHandler) return;
		visibilityHandler = () => {
			if (document.visibilityState === 'visible') startPolling();
			else stopPolling();
		};
		document.addEventListener('visibilitychange', visibilityHandler);
		if (document.visibilityState === 'visible') startPolling();
	},
	stop() {
		stopPolling();
		if (visibilityHandler) {
			document.removeEventListener('visibilitychange', visibilityHandler);
			visibilityHandler = undefined;
		}
	}
};
