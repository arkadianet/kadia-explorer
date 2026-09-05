const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Coarse relative time, e.g. "12 s ago", "3 min ago", "2 h ago", "5 d ago". */
export function relTime(msSinceEpoch: number, now: number = Date.now()): string {
	const delta = Math.max(0, now - msSinceEpoch);
	if (delta < MINUTE) return `${Math.floor(delta / SECOND)} s ago`;
	if (delta < HOUR) return `${Math.floor(delta / MINUTE)} min ago`;
	if (delta < DAY) return `${Math.floor(delta / HOUR)} h ago`;
	return `${Math.floor(delta / DAY)} d ago`;
}

function pad(n: number): string {
	return n.toString().padStart(2, '0');
}

/** Local "YYYY-MM-DD HH:MM:SS" timestamp. */
export function absTime(ms: number): string {
	const d = new Date(ms);
	const y = d.getFullYear();
	const mo = pad(d.getMonth() + 1);
	const day = pad(d.getDate());
	const h = pad(d.getHours());
	const mi = pad(d.getMinutes());
	const s = pad(d.getSeconds());
	return `${y}-${mo}-${day} ${h}:${mi}:${s}`;
}
