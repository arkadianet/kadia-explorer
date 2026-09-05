type Mode = 'dark' | 'light';

const KEY = 'xp-theme';

let current = $state<Mode>('dark');

function apply(m: Mode) {
	current = m;
	document.documentElement.dataset.theme = m;
}

export const theme = {
	get current() {
		return current;
	},
	init() {
		let saved: string | null = null;
		try {
			saved = localStorage.getItem(KEY);
		} catch {
			// localStorage may be unavailable (private mode, disabled storage)
		}
		// Dark unless light was explicitly chosen. The no-flash script in app.html applies the
		// same rule before first paint; the two must stay in step.
		apply(saved === 'light' ? 'light' : 'dark');
	},
	toggle() {
		const m: Mode = current === 'dark' ? 'light' : 'dark';
		apply(m);
		try {
			localStorage.setItem(KEY, m);
		} catch {
			// ignore persistence failures
		}
	}
};
