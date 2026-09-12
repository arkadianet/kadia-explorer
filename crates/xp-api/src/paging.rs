//! Ordinary paging policy and continuation foundations. Handler integration is M5 step 3.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    ImmutableMembership,
    CurrentState,
}

/// One entry per paged route/order; samples and history deliberately have no entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Blocks,
    GlobalSummaries,
    BlockSummaries,
    AddressSummaries,
    Transactions,
    AddressBoxes,
    TokenBoxes,
    TemplateBoxes,
    TokensNewest,
    TokensHolders,
    Holders,
    Richlist,
    RegisterBoxes,
    RentEligible,
}

impl Route {
    pub fn policy(self) -> Policy {
        match self {
            Self::Blocks
            | Self::GlobalSummaries
            | Self::BlockSummaries
            | Self::AddressSummaries => Policy::ImmutableMembership,
            Self::Transactions
            | Self::AddressBoxes
            | Self::TokenBoxes
            | Self::TemplateBoxes
            | Self::TokensNewest
            | Self::TokensHolders
            | Self::Holders
            | Self::Richlist
            | Self::RegisterBoxes
            | Self::RentEligible => Policy::CurrentState,
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn documented_route_matrix_is_exhaustive_and_classifies_enrichment() {
        let doc = include_str!("../../../docs/superpowers/2026-09-12-m5-route-policy-matrix.md");
        let immutable = [
            Route::Blocks,
            Route::GlobalSummaries,
            Route::BlockSummaries,
            Route::AddressSummaries,
        ];
        let current = [
            Route::Transactions,
            Route::AddressBoxes,
            Route::TokenBoxes,
            Route::TemplateBoxes,
            Route::TokensNewest,
            Route::TokensHolders,
            Route::Holders,
            Route::Richlist,
            Route::RegisterBoxes,
            Route::RentEligible,
        ];
        for (routes, policy, label) in [
            (
                &immutable[..],
                Policy::ImmutableMembership,
                "immutable membership",
            ),
            (&current[..], Policy::CurrentState, "current-state"),
        ] {
            for route in routes {
                assert_eq!(route.policy(), policy);
                let key = serde_json::to_value(route).unwrap();
                let row = doc
                    .lines()
                    .find(|line| line.starts_with(&format!("| `{}` |", key.as_str().unwrap())))
                    .unwrap();
                assert!(row.contains(label), "{row}");
            }
        }
        assert_eq!(
            doc.lines().filter(|line| line.starts_with("| `")).count(),
            14
        );
    }
}

use crate::ApiError;
use serde::{Deserialize, Serialize};
use xp_store::Reader;
use xp_types::Hash32;
use xp_wire::tree::blake2b256;

/// Limits apply before hex allocation / JSON parsing. No client-controlled collections.
pub const MAX_SNAPSHOT_BYTES: usize = 1024;
pub const MAX_SNAPSHOT_HEX: usize = MAX_SNAPSHOT_BYTES * 2;
const MAX_CURSOR_BYTES: usize = 85; // u64 decimal + ':' + 32-byte hex id

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    Asc,
    Desc,
}

/// Already normalized path/filter values. Resolve block heights to block ids in the Reader.
#[derive(Clone, Copy, Debug, Serialize)]
pub enum Filter {
    None,
    Entity(Hash32),
    Boxes { entity: Hash32, unspent: bool },
    Register { number: u8, value_hash: Hash32 },
}

#[derive(Clone, Debug)]
pub struct Binding {
    route: Route,
    order: Order,
    filter: Hash32,
}

impl Binding {
    pub fn new(route: Route, order: Order, filter: Filter) -> Result<Self, ApiError> {
        use Route::*;
        let valid_filter = match route {
            Blocks | GlobalSummaries | Transactions | TokensNewest | TokensHolders | Richlist
            | RentEligible => matches!(filter, Filter::None),
            BlockSummaries | AddressSummaries | Holders => matches!(filter, Filter::Entity(_)),
            AddressBoxes | TokenBoxes | TemplateBoxes => matches!(filter, Filter::Boxes { .. }),
            RegisterBoxes => matches!(filter, Filter::Register { number: 4..=9, .. }),
        };
        let valid_order = match route {
            Blocks | TokensNewest | TokensHolders | Holders | Richlist => order == Order::Desc,
            RentEligible => order == Order::Asc,
            _ => true,
        };
        if !valid_filter || !valid_order {
            return Err(invalid());
        }
        Ok(Self {
            route,
            order,
            filter: blake2b256(&serde_json::to_vec(&filter).map_err(|_| invalid())?),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub height: u32,
    #[serde(with = "hash_hex")]
    pub block_id: Hash32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    version: u8,
    route: Route,
    #[serde(with = "hash_hex")]
    filter: Hash32,
    order: Order,
    anchor: Anchor,
    #[serde(with = "hash_hex")]
    cursor_hash: Hash32,
}

mod hash_hex {
    use super::*;

    pub fn serialize<S: serde::Serializer>(
        hash: &Hash32,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(hash))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Hash32, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw.len() != 64 {
            return Err(serde::de::Error::custom("expected 32-byte hex hash"));
        }
        let mut hash = [0; 32];
        hex::decode_to_slice(raw, &mut hash).map_err(serde::de::Error::custom)?;
        Ok(hash)
    }
}

fn invalid() -> ApiError {
    ApiError::BadRequest("invalid snapshot or cursor/token binding".into())
}

fn changed() -> ApiError {
    ApiError::History {
        status: axum::http::StatusCode::CONFLICT,
        code: "snapshot_changed",
        detail: "The requested anchor changed or is unavailable; restart pagination.".into(),
    }
}

fn cursor_hash(cursor: &str) -> Result<Hash32, ApiError> {
    if cursor.is_empty() || cursor.len() > MAX_CURSOR_BYTES {
        return Err(invalid());
    }
    // Bind the existing exclusive cursor, never store its monetary ranking component.
    // The handler must still parse it with the unchanged route-specific cursor parser.
    Ok(blake2b256(cursor.as_bytes()))
}

fn decode(raw: &str) -> Result<Token, ApiError> {
    if raw.is_empty() || raw.len() > MAX_SNAPSHOT_HEX {
        return Err(invalid());
    }
    let bytes = hex::decode(raw).map_err(|_| invalid())?;
    let token: Token = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if token.version != 1 {
        return Err(invalid());
    }
    Ok(token)
}

fn observed(rd: &Reader) -> Result<Option<Anchor>, ApiError> {
    let Some(height) = rd.indexed_height()? else {
        return Ok(None);
    };
    // Partial seeds may have metadata but no stored header. Do not invent an id.
    Ok(rd.header_at(height)?.map(|header| Anchor {
        height,
        block_id: header.id,
    }))
}

/// Borrowed page context: validation, page reads and issuance use this exact Reader.
/// No public detached validator or constructor for a validated context exists.
/// Integration must apply the original anchor's store-derived membership bounds when
/// reading immutable pages; these bounds are intentionally absent from untrusted tokens.
pub struct PageReader<'a> {
    rd: &'a Reader,
    binding: &'a Binding,
    anchor: Option<Anchor>,
    observed: Option<Anchor>,
}

impl PageReader<'_> {
    pub fn reader(&self) -> &Reader {
        self.rd
    }
    pub fn anchor(&self) -> Option<Anchor> {
        self.anchor
    }
    pub fn observed(&self) -> Option<Anchor> {
        self.observed
    }

    /// Call with the page's unchanged next_cursor. No anchor means no continuation token.
    pub fn next_snapshot(&self, next_cursor: Option<&str>) -> Result<Option<String>, ApiError> {
        let (Some(anchor), Some(cursor)) = (self.anchor, next_cursor) else {
            return Ok(None);
        };
        let bytes = serde_json::to_vec(&Token {
            version: 1,
            route: self.binding.route,
            filter: self.binding.filter,
            order: self.binding.order,
            anchor,
            cursor_hash: cursor_hash(cursor)?,
        })
        .map_err(|_| invalid())?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid());
        }
        Ok(Some(hex::encode(bytes)))
    }
}

/// Strict foundation only; legacy and HTTP parameter/metadata integration is step 3.
/// Invoke inside `blocking`, after normalizing paths/filters and parsing the legacy cursor
/// in that closure. The callback reads through PageReader, never a fresh Store Reader.
/// Validates malformed/binding errors before any anchor lookups. Anchor checks perform
/// at most two header lookups plus bounded metadata reads, independent of token contents.
pub fn with_page<T>(
    rd: &Reader,
    binding: &Binding,
    snapshot: Option<&str>,
    cursor: Option<&str>,
    page: impl FnOnce(&PageReader<'_>) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let token = match (snapshot, cursor) {
        (None, None) => None,
        (Some(raw), Some(cursor)) => {
            let token = decode(raw)?;
            if token.route != binding.route
                || token.filter != binding.filter
                || token.order != binding.order
                || token.cursor_hash != cursor_hash(cursor)?
            {
                return Err(invalid());
            }
            Some(token)
        }
        _ => return Err(invalid()),
    };
    let observed = observed(rd)?;
    let anchor = if let Some(token) = token {
        let tip = observed.ok_or_else(changed)?;
        if token.anchor.height > tip.height
            || (binding.route.policy() == Policy::CurrentState && token.anchor != tip)
            || rd
                .header_at(token.anchor.height)?
                .is_none_or(|h| h.id != token.anchor.block_id)
        {
            return Err(changed());
        }
        Some(token.anchor)
    } else {
        observed
    };
    page(&PageReader {
        rd,
        binding,
        anchor,
        observed,
    })
}

#[cfg(test)]
mod token_tests {
    use super::*;
    use xp_store::Store;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("paging.redb")).unwrap();
        store.seed_for_tests(10, [10; 32]).unwrap();
        (dir, store)
    }

    fn binding(route: Route, order: Order, filter: Filter) -> Binding {
        Binding::new(route, order, filter).unwrap()
    }

    fn mint(rd: &Reader, binding: &Binding, cursor: &str) -> String {
        with_page(rd, binding, None, None, |page| {
            assert!(std::ptr::eq(rd, page.reader()));
            page.next_snapshot(Some(cursor))
        })
        .unwrap()
        .unwrap()
    }

    fn reject(rd: &Reader, binding: &Binding, token: Option<&str>, cursor: Option<&str>) {
        let result = with_page(rd, binding, token, cursor, |_| -> Result<(), ApiError> {
            panic!("rejected token must never invoke the page reader")
        });
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    fn rewrite(raw: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut value = serde_json::from_slice(&hex::decode(raw).unwrap()).unwrap();
        edit(&mut value);
        hex::encode(serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn route_order_and_filter_bindings_reject_in_both_directions() {
        let (_dir, store) = store();
        let rd = Reader::new(&store).unwrap();
        let base = binding(Route::GlobalSummaries, Order::Asc, Filter::None);
        let other_route = binding(Route::Transactions, Order::Asc, Filter::None);
        let other_order = binding(Route::GlobalSummaries, Order::Desc, Filter::None);
        let entity = binding(Route::AddressSummaries, Order::Asc, Filter::Entity([1; 32]));
        let other_entity = binding(Route::AddressSummaries, Order::Asc, Filter::Entity([2; 32]));
        let boxes = |entity, unspent| {
            binding(
                Route::AddressBoxes,
                Order::Asc,
                Filter::Boxes { entity, unspent },
            )
        };
        let register = |number, value_hash| {
            binding(
                Route::RegisterBoxes,
                Order::Asc,
                Filter::Register { number, value_hash },
            )
        };
        let pairs = [
            (base.clone(), other_route),
            (base, other_order),
            (entity, other_entity),
            (boxes([1; 32], false), boxes([1; 32], true)),
            (boxes([1; 32], false), boxes([2; 32], false)),
            (register(4, [1; 32]), register(5, [1; 32])),
            (register(4, [1; 32]), register(4, [2; 32])),
            (
                binding(Route::TokensNewest, Order::Desc, Filter::None),
                binding(Route::TokensHolders, Order::Desc, Filter::None),
            ),
        ];
        for (a, b) in pairs {
            for (source, target) in [(&a, &b), (&b, &a)] {
                let token = mint(&rd, source, "42");
                with_page(&rd, source, Some(&token), Some("42"), |_| Ok(())).unwrap();
                reject(&rd, target, Some(&token), Some("42"));
                reject(&rd, source, Some(&token), Some("43"));
            }
        }
    }

    #[test]
    fn strict_parser_and_exact_size_bound() {
        let (_dir, store) = store();
        let rd = Reader::new(&store).unwrap();
        let binding = binding(Route::Blocks, Order::Desc, Filter::None);
        let token = mint(&rd, &binding, "42");
        let maximal = Token {
            version: 1,
            route: Route::AddressSummaries,
            order: Order::Desc,
            filter: [255; 32],
            anchor: Anchor {
                height: u32::MAX,
                block_id: [255; 32],
            },
            cursor_hash: [255; 32],
        };
        let maximal_bytes = serde_json::to_vec(&maximal).unwrap();
        assert!(maximal_bytes.len() <= MAX_SNAPSHOT_BYTES);
        assert_eq!(
            decode(&hex::encode(maximal_bytes)).unwrap().anchor,
            maximal.anchor
        );
        for version in [0, 2, 255] {
            reject(
                &rd,
                &binding,
                Some(&rewrite(&token, |v| v["version"] = version.into())),
                Some("42"),
            );
        }
        for raw in [
            "".to_owned(),
            "0".into(),
            "zz".into(),
            hex::encode(b"{}"),
            rewrite(&token, |v| v["extra"] = true.into()),
            rewrite(&token, |v| v["anchor"]["extra"] = true.into()),
            rewrite(&token, |v| v["route"] = "unknown".into()),
            rewrite(&token, |v| v["order"] = "unknown".into()),
            rewrite(&token, |v| {
                v.as_object_mut().unwrap().remove("cursor_hash");
            }),
        ] {
            reject(&rd, &binding, Some(&raw), Some("42"));
        }
        let mut bytes = hex::decode(&token).unwrap();
        bytes.resize(MAX_SNAPSHOT_BYTES, b' ');
        let boundary = hex::encode(&bytes);
        assert_eq!(boundary.len(), MAX_SNAPSHOT_HEX);
        with_page(&rd, &binding, Some(&boundary), Some("42"), |_| Ok(())).unwrap();
        bytes.push(b' ');
        reject(&rd, &binding, Some(&hex::encode(bytes)), Some("42"));
        reject(&rd, &binding, Some(&(boundary + "0")), Some("42"));
        reject(&rd, &binding, None, Some("42"));
        reject(&rd, &binding, Some(&token), None);
        assert!(cursor_hash("").is_err());
        assert!(cursor_hash(&"1".repeat(MAX_CURSOR_BYTES)).is_ok());
        assert!(cursor_hash(&"1".repeat(MAX_CURSOR_BYTES + 1)).is_err());
        assert!(Binding::new(Route::Blocks, Order::Asc, Filter::None).is_err());
        assert!(Binding::new(Route::Holders, Order::Desc, Filter::None).is_err());
        assert!(Binding::new(
            Route::RegisterBoxes,
            Order::Asc,
            Filter::Register {
                number: 3,
                value_hash: [0; 32]
            }
        )
        .is_err());
    }

    fn conflict(rd: &Reader, binding: &Binding, token: &str) {
        let result = with_page(
            rd,
            binding,
            Some(token),
            Some("42"),
            |_| -> Result<(), ApiError> { panic!("invalid anchor must never invoke page reads") },
        );
        assert!(matches!(
            result,
            Err(ApiError::History {
                status: axum::http::StatusCode::CONFLICT,
                code: "snapshot_changed",
                ..
            })
        ));
    }

    #[test]
    fn validation_and_page_reads_share_snapshot_across_append_and_reorg() {
        let (_dir, store) = store();
        let old = Reader::new(&store).unwrap();
        let immutable = binding(Route::Blocks, Order::Desc, Filter::None);
        let current = binding(Route::Transactions, Order::Asc, Filter::None);
        let immutable_token = mint(&old, &immutable, "42");
        let current_token = mint(&old, &current, "42");
        store.seed_for_tests(11, [11; 32]).unwrap();
        for (binding, token) in [(&immutable, &immutable_token), (&current, &current_token)] {
            with_page(&old, binding, Some(token), Some("42"), |page| {
                assert!(std::ptr::eq(page.reader(), &old));
                assert_eq!(page.reader().indexed_height()?, Some(10));
                assert_eq!(page.observed(), page.anchor());
                assert!(page.reader().header_at(11)?.is_none());
                assert_eq!(
                    page.next_snapshot(Some("42"))?.as_deref(),
                    Some(token.as_str())
                );
                Ok(())
            })
            .unwrap();
        }
        let appended = Reader::new(&store).unwrap();
        conflict(&appended, &current, &current_token);
        with_page(
            &appended,
            &immutable,
            Some(&immutable_token),
            Some("42"),
            |page| {
                assert_eq!(page.anchor().unwrap().height, 10);
                assert_eq!(page.observed().unwrap().height, 11);
                assert_eq!(page.reader().indexed_height()?, Some(11));
                Ok(())
            },
        )
        .unwrap();
        // Replace the anchor at the same height, then move the tip below it.
        store.seed_for_tests(10, [99; 32]).unwrap();
        let replaced = Reader::new(&store).unwrap();
        conflict(&replaced, &immutable, &immutable_token);
        conflict(&replaced, &current, &current_token);
        store.seed_for_tests(9, [9; 32]).unwrap();
        conflict(&Reader::new(&store).unwrap(), &immutable, &immutable_token);
        // A pinned reader still validates against its own canonical header.
        with_page(&old, &immutable, Some(&immutable_token), Some("42"), |_| {
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn forged_anchors_reject_and_empty_store_cannot_issue() {
        let (_dir, store) = store();
        let rd = Reader::new(&store).unwrap();
        let binding = binding(Route::Blocks, Order::Desc, Filter::None);
        let token = mint(&rd, &binding, "42");
        for height in [0, 9, 11, u32::MAX] {
            conflict(
                &rd,
                &binding,
                &rewrite(&token, |v| v["anchor"]["height"] = height.into()),
            );
        }
        conflict(
            &rd,
            &binding,
            &rewrite(&token, |v| {
                v["anchor"]["block_id"] = serde_json::Value::String("00".repeat(32))
            }),
        );
        let dir = tempfile::tempdir().unwrap();
        let empty = Store::open(&dir.path().join("empty.redb")).unwrap();
        let empty_rd = Reader::new(&empty).unwrap();
        with_page(&empty_rd, &binding, None, None, |page| {
            assert_eq!(page.anchor(), None);
            assert_eq!(page.next_snapshot(Some("42"))?, None);
            Ok(())
        })
        .unwrap();
        conflict(&empty_rd, &binding, &token);
        with_page(&rd, &binding, None, None, |page| {
            assert_eq!(page.next_snapshot(None)?, None);
            Ok(())
        })
        .unwrap();
    }
}
