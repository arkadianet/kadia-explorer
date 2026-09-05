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

pub const ALL: [Tbl; 15] = [
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
];

pub const META_INDEXED_HEIGHT: &[u8] = b"indexed_height";
pub const META_NEXT_BOX_GIDX: &[u8] = b"next_box_gidx";
pub const META_NEXT_TX_GIDX: &[u8] = b"next_tx_gidx";
pub const META_SCHEMA: &[u8] = b"schema_version";
pub const SCHEMA_VERSION: u32 = 1;
