//! Resolution of authored descendant names to canonical UI node identities.

use crate::assets::source_document::ui::model::SourceUiNode;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use openzt2_game_data::AssetId;

pub(super) fn resolve_owned_authored_ui_descendant_node_id(
    node: &SourceUiNode,
    direct_children: &[u32],
    target: &str,
    output: &BuildOutput,
) -> Option<AssetId> {
    node.children
        .iter()
        .zip(direct_children)
        .find_map(|(child, &child_index)| {
            if child
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(target))
            {
                return Some(output.nodes[child_index as usize].id);
            }
            let links = &output.nodes[child_index as usize].children;
            resolve_owned_authored_ui_descendant_node_id(child, links, target, output)
        })
}
