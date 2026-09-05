use blake2::{digest::consts::U32, Blake2b, Digest};
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use xp_types::{Hash32, TreeHash};

use crate::WireError;

pub fn blake2b256(data: &[u8]) -> Hash32 {
    let mut h = Blake2b::<U32>::new();
    h.update(data);
    h.finalize().into()
}

pub fn tree_hash(tree_bytes: &[u8]) -> TreeHash {
    TreeHash(blake2b256(tree_bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    P2pk,
    P2s,
    Other,
}

#[derive(Debug, Clone)]
pub struct TreeInfo {
    pub template_hash: Hash32,
    pub address: String,
    pub kind: TreeKind,
}

pub fn template_hash_of(tree_bytes: &[u8]) -> Result<Hash32, WireError> {
    let tree =
        ErgoTree::sigma_parse_bytes(tree_bytes).map_err(|e| WireError::Tree(e.to_string()))?;
    let tmpl = tree
        .template_bytes()
        .map_err(|e| WireError::Tree(e.to_string()))?;
    Ok(blake2b256(&tmpl))
}

pub fn tree_info(tree_bytes: &[u8]) -> Result<TreeInfo, WireError> {
    let tree =
        ErgoTree::sigma_parse_bytes(tree_bytes).map_err(|e| WireError::Tree(e.to_string()))?;
    let tmpl = tree
        .template_bytes()
        .map_err(|e| WireError::Tree(e.to_string()))?;
    let addr =
        Address::recreate_from_ergo_tree(&tree).map_err(|e| WireError::Tree(e.to_string()))?;
    let kind = match &addr {
        Address::P2Pk(_) => TreeKind::P2pk,
        Address::P2S(_) => TreeKind::P2s,
        _ => TreeKind::Other,
    };
    let address = AddressEncoder::new(NetworkPrefix::Mainnet).address_to_str(&addr);
    Ok(TreeInfo {
        template_hash: blake2b256(&tmpl),
        address,
        kind,
    })
}
