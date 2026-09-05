import { describe, it, expect } from 'vitest';
import { classify, routeFor } from '$lib/search/classify';

describe('classify', () => {
	it('height', () => expect(classify(' 1866000 ')).toEqual({ kind: 'height', value: 1866000 }));
	it('hex32 lowercased', () =>
		expect(classify('AA44EF6A6C08D0B198D65B762ABB0181E6AD995654116FBEDAA1D3D3EB95A4D6')).toEqual({
			kind: 'hex32',
			value: 'aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6'
		}));
	it('address', () =>
		expect(classify('9i5FJNkbtZH8kcS129ny71wLBK7cKhJ7ixLpLTZoirjXRFhfvs').kind).toBe('address'));
	it('unknown', () => {
		expect(classify('hello').kind).toBe('unknown');
		expect(classify('0x1234').kind).toBe('unknown');
		expect(classify('').kind).toBe('unknown');
	});
});

describe('routeFor', () => {
	it('height routes straight to the block page', () => {
		expect(routeFor({ kind: 'height', value: 1866000 })).toBe('/blocks/1866000');
	});

	it('address routes straight to the address page', () => {
		expect(
			routeFor({ kind: 'address', value: '9i5FJNkbtZH8kcS129ny71wLBK7cKhJ7ixLpLTZoirjXRFhfvs' })
		).toBe('/address/9i5FJNkbtZH8kcS129ny71wLBK7cKhJ7ixLpLTZoirjXRFhfvs');
	});

	it('hex32 needs the API to disambiguate block/tx/box', () => {
		expect(
			routeFor({
				kind: 'hex32',
				value: 'aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6'
			})
		).toBeNull();
	});

	it('unknown has no route', () => {
		expect(routeFor({ kind: 'unknown', value: 'hello' })).toBeNull();
	});
});
