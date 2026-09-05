type Mode = 'dark' | 'light';

const KEY = 'xp-theme';

function systemMode(): Mode {
	return matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

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
