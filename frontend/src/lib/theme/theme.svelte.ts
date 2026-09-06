type Mode = 'dark' | 'light';

const KEY = 'xp-theme';

let current = $state<Mode>('light');

function apply(m: Mode) {
	current = m;
	document.documentElement.dataset.theme = m;
}

function systemMode(): Mode {
	try {
		return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
	} catch {
		return 'light';
	}
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
		// The system preference decides unless the toggle has been used. The no-flash script in
		// app.html applies the same rule before first paint; the two must stay in step.
		apply(saved === 'light' || saved === 'dark' ? saved : systemMode());
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
