//! Source-node pruning by selected expansion-pack availability.

use std::collections::BTreeSet;

use crate::assets::source_document::ui::model::SourceUiNode;

/// Resolves the original loader's `xPack` construction guard before any
/// templates, skins, roles, or native records are collected. A rejected parent
/// rejects its complete subtree, matching the source loader's early return.
pub(super) fn remove_ui_source_nodes_requiring_unavailable_expansion_packs(
    node: &mut SourceUiNode,
    available_expansion_packs: &BTreeSet<u32>,
) -> bool {
    if node
        .x_pack
        .is_some_and(|required| !available_expansion_packs.contains(&required))
    {
        return false;
    }
    node.x_pack = None;
    node.children.retain_mut(|child| {
        remove_ui_source_nodes_requiring_unavailable_expansion_packs(
            child,
            available_expansion_packs,
        )
    });
    true
}
