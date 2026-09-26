//! Stable source-derived geometry, material, and skeleton identity.

use super::super::{
    blue_fang_bfb_source::document::BlueFangBfbDocument,
    netimmerse_nif_source::document_source_types::NetImmerseNifDocument,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::assets::model_source) enum NativeMaterialReference<'a> {
    BlueFangMaterialName(&'a str),
    NetImmerseGeometry { document: &'a str, block: u32 },
}

impl NativeMaterialReference<'_> {
    pub(in crate::assets::model_source) fn asset_key(self) -> String {
        match self {
            Self::BlueFangMaterialName(name) => name.to_owned(),
            Self::NetImmerseGeometry { document, block } => {
                format!("{document}.__material/nif_{block:08x}")
            }
        }
    }
}

pub(in crate::assets::model_source) fn blue_fang_geometry_virtual_path(
    document: &BlueFangBfbDocument,
    node_index: usize,
) -> String {
    format!(
        "{}#geometry/bfb_{node_index:08x}",
        document.source_path.as_str()
    )
}

pub(in crate::assets::model_source) fn netimmerse_geometry_virtual_path(
    document: &NetImmerseNifDocument,
    block_index: u32,
) -> String {
    format!(
        "{}#geometry/nif_{block_index:08x}",
        document.source_path.as_str()
    )
}
