use redb::TableDefinition;

pub type Tbl = TableDefinition<'static, &'static [u8], &'static [u8]>;

pub const META: Tbl = TableDefinition::new("meta");
pub const HEADERS: Tbl = TableDefinition::new("headers");
pub const HEADER_BY_ID: Tbl = TableDefinition::new("header_by_id");
pub const TXS: Tbl = TableDefinition::new("txs");
pub const TX_BY_GIDX: Tbl = TableDefinition::new("tx_by_gidx");
pub const BOXES: Tbl = TableDefinition::new("boxes");
pub const BOX_BY_GIDX: Tbl = TableDefinition::new("box_by_gidx");
pub const ERGO_TREES: Tbl = TableDefinition::new("ergo_trees");
pub const TREE_BOXES: Tbl = TableDefinition::new("tree_boxes");
pub const TREE_UNSPENT: Tbl = TableDefinition::new("tree_unspent");
pub const TREE_TXS: Tbl = TableDefinition::new("tree_txs");
pub const TREE_BALANCE: Tbl = TableDefinition::new("tree_balance");
pub const RICH: Tbl = TableDefinition::new("rich");
pub const RENT_MATURES: Tbl = TableDefinition::new("rent_matures");
pub const UNDO: Tbl = TableDefinition::new("undo");

/// `template_hash -> TemplateRow`.
pub const TEMPLATES: Tbl = TableDefinition::new("templates");
/// `(template_hash, gidx) -> ()`: every box carrying this ergo-tree template.
pub const TEMPLATE_BOXES: Tbl = TableDefinition::new("template_boxes");
/// `(template_hash, gidx) -> ()`: the subset of `TEMPLATE_BOXES` still unspent.
pub const TEMPLATE_UNSPENT: Tbl = TableDefinition::new("template_unspent");
/// `token_id -> TokenRow`.
pub const TOKENS: Tbl = TableDefinition::new("tokens");
/// mint box gidx (`u64` BE) `-> token_id`: newest-first token listing.
pub const TOKENS_BY_GIDX: Tbl = TableDefinition::new("tokens_by_gidx");
/// `(holder_count u64 BE, token_id) -> ()`.
pub const TOKENS_BY_HOLDERS: Tbl = TableDefinition::new("tokens_by_holders");
/// `(token_id, gidx) -> ()`: every box carrying this token.
pub const TOKEN_BOXES: Tbl = TableDefinition::new("token_boxes");
/// `(token_id, gidx) -> ()`: the subset of `TOKEN_BOXES` still unspent.
pub const TOKEN_UNSPENT: Tbl = TableDefinition::new("token_unspent");
/// `(token_id, amount BE, tree) -> ()`: richest-holders listing for a token.
pub const TOKEN_HOLDERS: Tbl = TableDefinition::new("token_holders");
/// `(token_id, tree) -> u64 BE`: a tree's current balance of a token.
pub const TOKEN_HOLDER_AMT: Tbl = TableDefinition::new("token_holder_amt");
/// `(reg u8, blake2b256(raw value bytes), gidx) -> ()`: register-value index.
pub const REGISTER_IDX: Tbl = TableDefinition::new("register_idx");

pub const ALL: [Tbl; 26] = [
    META,
    HEADERS,
    HEADER_BY_ID,
    TXS,
    TX_BY_GIDX,
    BOXES,
    BOX_BY_GIDX,
    ERGO_TREES,
    TREE_BOXES,
    TREE_UNSPENT,
    TREE_TXS,
    TREE_BALANCE,
    RICH,
    RENT_MATURES,
    UNDO,
    TEMPLATES,
    TEMPLATE_BOXES,
    TEMPLATE_UNSPENT,
    TOKENS,
    TOKENS_BY_GIDX,
    TOKENS_BY_HOLDERS,
    TOKEN_BOXES,
    TOKEN_UNSPENT,
    TOKEN_HOLDERS,
    TOKEN_HOLDER_AMT,
    REGISTER_IDX,
];

pub const META_INDEXED_HEIGHT: &[u8] = b"indexed_height";
pub const META_NEXT_BOX_GIDX: &[u8] = b"next_box_gidx";
pub const META_NEXT_TX_GIDX: &[u8] = b"next_tx_gidx";
pub const META_SCHEMA: &[u8] = b"schema_version";
/// Set to `[1]` once the chain-spec genesis boxes have been written into the store by
/// `Store::seed_genesis`. Distinct from `META_INDEXED_HEIGHT`: genesis boxes belong to no
/// block, so seeding them leaves the store's indexed height untouched (still `None`).
pub const META_GENESIS_SEEDED: &[u8] = b"genesis_seeded";
/// The 32-byte blake2b256 ergo tree hash of the chain-spec emission box, written by
/// `Store::seed_genesis`. Absent on a store that never seeded genesis (a partial store), in
/// which case the API simply cannot label emission boxes.
pub const META_EMISSION_TREE_HASH: &[u8] = b"emission_tree_hash";
pub const SCHEMA_VERSION: u32 = 2;

/// Present only on a store that began indexing later than chain genesis (written by
/// `Store::seed_for_tests` in tests; a real full sync from height 1 never sets it). Its
/// value is the height the store was seeded at. When set, `apply_batch` tolerates missing
/// input boxes, since boxes created before the seed point were never indexed. It is the only
/// such tolerance: a store synced from height 1 seeds the chain-spec genesis boxes instead.
pub const META_PARTIAL_FROM: &[u8] = b"partial_from";
