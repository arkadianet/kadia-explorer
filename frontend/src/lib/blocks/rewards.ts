import { apiGet } from '$lib/api/client';

export interface RewardBreakdown {
	block_id: string;
	height: number;
	basis: 'observed_eip27_reward_box' | 'unsupported';
	gross_reward: string | null;
	reemission_obligation: string | null;
	miner_subsidy: string | null;
	transaction_fees: string;
	reward_box_id: string | null;
	transaction_id: string | null;
	note: string;
}
export const blockRewards = (id: string, f?: typeof fetch) =>
	apiGet<RewardBreakdown>(`/blocks/${encodeURIComponent(id)}/rewards`, undefined, f);
