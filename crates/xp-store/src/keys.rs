use xp_types::{Gidx, Hash32};

pub fn k_u32(h: u32) -> [u8; 4] {
    h.to_be_bytes()
}

pub fn k_u64(g: u64) -> [u8; 8] {
    g.to_be_bytes()
}

pub fn k_hash_gidx(h: &Hash32, g: Gidx) -> [u8; 40] {
    let mut k = [0u8; 40];
    k[..32].copy_from_slice(h);
    k[32..].copy_from_slice(&g.to_be_bytes());
    k
}

pub fn k_rich(nano: u64, tree: &Hash32) -> [u8; 40] {
    let mut k = [0u8; 40];
    k[..8].copy_from_slice(&nano.to_be_bytes());
    k[8..].copy_from_slice(tree);
    k
}

pub fn k_rent(mature: u32, g: Gidx) -> [u8; 12] {
    let mut k = [0u8; 12];
    k[..4].copy_from_slice(&mature.to_be_bytes());
    k[4..].copy_from_slice(&g.to_be_bytes());
    k
}

/// Extracts the trailing 8-byte big-endian gidx from a composite key (e.g. `k_hash_gidx` or
/// `k_rent` output).
pub fn gidx_of_composite(k: &[u8]) -> Gidx {
    let mut b = [0u8; 8];
    b.copy_from_slice(&k[k.len() - 8..]);
    u64::from_be_bytes(b)
}

/// Half-open range `[prefix.., prefix+1..)` covering every key that starts with `prefix`.
///
/// If `prefix` is all `0xFF` bytes it cannot be incremented; the upper bound is then
/// `prefix` with an extra `0xFF` byte appended, which is a documented limitation (hashes
/// used as prefixes are never all-`0xFF`).
pub fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let lo = prefix.to_vec();
    let mut hi = prefix.to_vec();
    for i in (0..hi.len()).rev() {
        if hi[i] != 0xFF {
            hi[i] += 1;
            hi.truncate(i + 1);
            return (lo, hi);
        }
    }
    hi.push(0xFF);
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_keys_order_by_gidx_within_hash() {
        let h = [7u8; 32];
        assert!(k_hash_gidx(&h, 1) < k_hash_gidx(&h, 2));
        assert!(k_hash_gidx(&[6u8; 32], u64::MAX) < k_hash_gidx(&h, 0));
    }

    #[test]
    fn rich_key_orders_by_balance() {
        assert!(k_rich(1, &[0; 32]) < k_rich(2, &[0; 32]));
    }

    #[test]
    fn prefix_range_is_half_open() {
        let (lo, hi) = prefix_range(&[0xAA, 0xFF]);
        assert_eq!(lo, vec![0xAA, 0xFF]);
        assert_eq!(hi, vec![0xAB]);
        let (_, hi2) = prefix_range(&[0xFF, 0xFF]);
        assert_eq!(hi2, vec![0xFF, 0xFF, 0xFF]); // cannot increment: sentinel = prefix + 0xFF (documented limitation; hashes never all-0xFF)
    }
}
