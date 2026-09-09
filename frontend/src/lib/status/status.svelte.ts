import { health } from './health';
import { api } from '$lib/api/endpoints';
import type { StatusDto } from '$lib/api/types';

const POLL_MS = 5_000;

let current = $state<StatusDto | null>(null);
let error = $state<unknown>(null);
let lastSuccess = $state<number | null>(null);
let now = $state(Date.now());
let inFlight = false;
let timer: ReturnType<typeof setInterval> | undefined;
let visibilityHandler: (() => void) | undefined;

async function poll(): Promise<void> {
	now = Date.now();
	if (inFlight) return;
	inFlight = true;
	try {
		current = await api.status();
		lastSuccess = Date.now();
		now = lastSuccess;
		error = null;
	} catch (e) {
		error = e;
	} finally {
		inFlight = false;
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
	get health() {
		return health(current, { lastSuccess, now, error });
	},
	get lastSuccess() {
		return lastSuccess;
	},
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
