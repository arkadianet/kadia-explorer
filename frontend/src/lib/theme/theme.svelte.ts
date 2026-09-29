type Mode = 'dark' | 'light';
export type Appearance = 'original' | 'aurora' | 'atelier' | 'prism';
export type Density = 'standard' | 'compact';

const KEY = 'xp-theme';
const APPEARANCE_KEY = 'xp-appearance';

let current = $state<Mode>('light');
let currentAppearance = $state<Appearance>('prism');
let currentDensity = $state<Density>('compact');

function appearanceOrDefault(value: string | null): Appearance {
	return value === 'original' || value === 'aurora' || value === 'atelier' || value === 'prism'
		? value
		: 'prism';
}

function applyAppearance(value: Appearance) {
	currentAppearance = value;
	document.documentElement.dataset.appearance = value;
}

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

export const appearance = {
	get current() {
		return currentAppearance;
	},
	init() {
		let saved: string | null = null;
		try {
			saved = localStorage.getItem(APPEARANCE_KEY);
		} catch {
			// The default still works when browser storage is unavailable.
		}
		// Keep validation and the default in step with app.html's pre-paint script.
		applyAppearance(appearanceOrDefault(saved));
	},
	set(value: Appearance) {
		const selected = appearanceOrDefault(value);
		applyAppearance(selected);
		try {
			localStorage.setItem(APPEARANCE_KEY, selected);
		} catch {
			// The current session can still use the selected appearance.
		}
	}
};

export const density = {
	get current() {
		return currentDensity;
	},
	init() {
		let saved: string | null = null;
		try {
			saved = localStorage.getItem('xp-density');
		} catch {
			// Compact density remains available without browser storage.
		}
		currentDensity = saved === 'standard' ? 'standard' : 'compact';
		document.documentElement.dataset.density = currentDensity;
	},
	set(value: Density) {
		currentDensity = value === 'standard' ? 'standard' : 'compact';
		document.documentElement.dataset.density = currentDensity;
		try {
			localStorage.setItem('xp-density', currentDensity);
		} catch {
			// The current session can still use the selected density.
		}
	}
};
