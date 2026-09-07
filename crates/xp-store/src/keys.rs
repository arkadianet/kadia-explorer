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

/// `prefix ++ gidx u64 BE` for a prefix of any width: the 32-byte hash of a
/// `k_hash_gidx` key, or the 33-byte `(reg, value_hash)` head of a `k_register` one. Used by
/// the readers' generic pager, which must build a bound key without knowing the width.
///
/// The pager's `Asc` bound is `k_prefix_gidx(prefix, cursor + 1)` with a saturating add, so a
/// cursor of `u64::MAX` would return that row again instead of ending the page — unreachable
/// in practice, since a gidx is a chain-order counter over boxes and transactions.
pub fn k_prefix_gidx(prefix: &[u8], g: Gidx) -> Vec<u8> {
    let mut k = Vec::with_capacity(prefix.len() + 8);
    k.extend_from_slice(prefix);
    k.extend_from_slice(&g.to_be_bytes());
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

/// `TOKEN_HOLDERS` key: `(token_id, amount u64 BE, tree)`. Orders by token first, then by
/// amount within a token, so a prefix scan over `token` yields holders from smallest to
/// largest amount.
pub fn k_token_holder(token: &Hash32, amount: u64, tree: &Hash32) -> [u8; 72] {
    let mut k = [0u8; 72];
    k[..32].copy_from_slice(token);
    k[32..40].copy_from_slice(&amount.to_be_bytes());
    k[40..].copy_from_slice(tree);
    k
}

/// `TOKEN_HOLDER_AMT` key: `(token_id, tree)`.
pub fn k_token_tree(token: &Hash32, tree: &Hash32) -> [u8; 64] {
    let mut k = [0u8; 64];
    k[..32].copy_from_slice(token);
    k[32..].copy_from_slice(tree);
    k
}

/// `TOKENS_BY_HOLDERS` key: `(count u64 BE, id)`. Orders by count first, so a prefix/range
/// scan yields ids from smallest to largest count.
pub fn k_by_count(count: u64, id: &Hash32) -> [u8; 40] {
    let mut k = [0u8; 40];
    k[..8].copy_from_slice(&count.to_be_bytes());
    k[8..].copy_from_slice(id);
    k
}

/// `REGISTER_IDX` key: `(reg u8, blake2b256(raw value bytes), gidx u64 BE)`.
pub fn k_register(reg: u8, value_hash: &Hash32, g: Gidx) -> [u8; 41] {
    let mut k = [0u8; 41];
    k[0] = reg;
    k[1..33].copy_from_slice(value_hash);
    k[33..].copy_from_slice(&g.to_be_bytes());
    k
}

/// Extracts the trailing 8-byte big-endian gidx from a composite key (e.g. `k_hash_gidx` or
/// `k_rent` output). A key shorter than 8 bytes cannot have been written by this module, so
/// it is [`StoreError::Corrupt`] rather than a panic.
pub fn gidx_of_composite(k: &[u8]) -> Result<Gidx, crate::StoreError> {
    let tail = k
        .get(k.len().saturating_sub(8)..)
        .filter(|t| t.len() == 8)
        .ok_or(crate::StoreError::Corrupt(
            "composite key shorter than 8 bytes",
        ))?;
    let mut b = [0u8; 8];
    b.copy_from_slice(tail);
    Ok(u64::from_be_bytes(b))
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
    fn gidx_of_composite_rejects_a_short_key() {
        assert_eq!(gidx_of_composite(&k_hash_gidx(&[3u8; 32], 42)).unwrap(), 42);
        assert_eq!(gidx_of_composite(&k_rent(7, 42)).unwrap(), 42);
        assert!(matches!(
            gidx_of_composite(&[0u8; 7]),
            Err(crate::StoreError::Corrupt(
                "composite key shorter than 8 bytes"
            ))
        ));
    }

    #[test]
    fn prefix_gidx_matches_the_fixed_width_key_builders() {
        let h = [7u8; 32];
        assert_eq!(k_prefix_gidx(&h, 42), k_hash_gidx(&h, 42).to_vec());
        let mut head = vec![4u8];
        head.extend_from_slice(&[3u8; 32]);
        assert_eq!(
            k_prefix_gidx(&head, 42),
            k_register(4, &[3u8; 32], 42).to_vec()
        );
    }

    #[test]
    fn rich_key_orders_by_balance() {
        assert!(k_rich(1, &[0; 32]) < k_rich(2, &[0; 32]));
    }

    #[test]
    fn token_holder_key_orders_by_amount_within_a_token() {
        let token = [1u8; 32];
        assert!(k_token_holder(&token, 1, &[0; 32]) < k_token_holder(&token, 2, &[0; 32]));
        // A different token entirely sorts by token first, regardless of amount.
        assert!(
            k_token_holder(&[0u8; 32], u64::MAX, &[0; 32]) < k_token_holder(&token, 0, &[0; 32])
        );
        // Within the same token and amount, orders by tree.
        assert!(k_token_holder(&token, 5, &[1; 32]) < k_token_holder(&token, 5, &[2; 32]));
    }

    #[test]
    fn token_tree_key_orders_by_token_then_tree() {
        let token = [1u8; 32];
        assert!(k_token_tree(&token, &[1; 32]) < k_token_tree(&token, &[2; 32]));
        assert!(k_token_tree(&[0u8; 32], &[9; 32]) < k_token_tree(&token, &[0; 32]));
    }

    #[test]
    fn by_count_key_orders_by_count() {
        assert!(k_by_count(1, &[0; 32]) < k_by_count(2, &[0; 32]));
        assert!(k_by_count(1, &[1; 32]) < k_by_count(1, &[2; 32]));
        assert!(k_by_count(0, &[0xFF; 32]) < k_by_count(1, &[0; 32]));
    }

    #[test]
    fn register_key_orders_by_reg_then_value_hash_then_gidx() {
        let h = [3u8; 32];
        assert!(k_register(0, &h, 1) < k_register(0, &h, 2));
        assert!(k_register(0, &h, u64::MAX) < k_register(1, &h, 0));
        assert!(k_register(0, &[1; 32], 0) < k_register(0, &[2; 32], 0));
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
