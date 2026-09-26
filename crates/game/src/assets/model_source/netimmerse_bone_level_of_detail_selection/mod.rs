//! Static selection of the active bone LOD authored by a NetImmerse model.

use std::collections::BTreeMap;

use super::netimmerse_nif_source::{
    block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
};

pub(super) struct NetImmerseSelectedBoneLevelOfDetail {
    node_visibility_by_block: BTreeMap<i32, bool>,
    shape_skin_instance_by_block: BTreeMap<i32, Option<i32>>,
}

impl NetImmerseSelectedBoneLevelOfDetail {
    pub(super) fn from_document(document: &NetImmerseNifDocument) -> Self {
        let mut selection = Self {
            node_visibility_by_block: BTreeMap::new(),
            shape_skin_instance_by_block: BTreeMap::new(),
        };
        document.blocks().for_each(|block| {
            let NetImmerseNifBlockPayload::NiBoneLODController(controller) = &block.payload else {
                return;
            };
            controller
                .node_groups
                .iter()
                .enumerate()
                .for_each(|(ordinal, nodes)| {
                    let visible = usize::try_from(controller.lod).ok() == Some(ordinal);
                    nodes.iter().for_each(|node| {
                        selection.node_visibility_by_block.insert(*node, visible);
                    });
                });
            controller.shape_refs.iter().for_each(|shape| {
                selection.shape_skin_instance_by_block.insert(*shape, None);
            });
            usize::try_from(controller.lod)
                .ok()
                .and_then(|ordinal| controller.shape_groups.get(ordinal))
                .into_iter()
                .flatten()
                .for_each(|shape| {
                    selection
                        .shape_skin_instance_by_block
                        .insert(shape.shape_ref, Some(shape.skin_instance_ref));
                });
        });
        selection
    }

    pub(super) fn scene_block_is_visible(&self, block: i32) -> bool {
        self.node_visibility_by_block
            .get(&block)
            .copied()
            .unwrap_or(true)
            && self
                .shape_skin_instance_by_block
                .get(&block)
                .is_none_or(Option::is_some)
    }

    pub(super) fn selected_or_authored_scene_block_visibility(
        &self,
        block: i32,
        authored_visibility: bool,
    ) -> bool {
        self.shape_skin_instance_by_block.get(&block).map_or_else(
            || {
                self.node_visibility_by_block
                    .get(&block)
                    .copied()
                    .unwrap_or(authored_visibility)
            },
            Option::is_some,
        )
    }

    pub(super) fn selected_shape_skin_instance(
        &self,
        block: i32,
        authored_skin_instance: i32,
    ) -> Option<i32> {
        self.shape_skin_instance_by_block
            .get(&block)
            .copied()
            .unwrap_or(Some(authored_skin_instance))
    }
}
