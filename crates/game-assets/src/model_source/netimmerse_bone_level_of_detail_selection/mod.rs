//! Authored bone LOD variants and the initial tier of a NetImmerse model.

use std::collections::BTreeMap;

use super::netimmerse_nif_source::{
    block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
};

pub(super) struct NetImmerseBoneLod {
    pub(super) controllers: BTreeMap<u32, NetImmerseBoneLodController>,
    pub(super) nodes: BTreeMap<i32, NetImmerseNodeBoneLod>,
    pub(super) shapes: BTreeMap<i32, NetImmerseShapeBoneLod>,
}

pub(super) struct NetImmerseBoneLodController {
    pub(super) target_ref: i32,
    pub(super) initial: u16,
    pub(super) tier_count: u16,
}

pub(super) struct NetImmerseNodeBoneLod {
    pub(super) controller: u32,
    pub(super) ordinal: u16,
    pub(super) initial: u16,
}

pub(super) struct NetImmerseShapeBoneLod {
    pub(super) controller: u32,
    pub(super) initial: u16,
    pub(super) skins: BTreeMap<u16, i32>,
}

impl NetImmerseBoneLod {
    pub(super) fn from_document(document: &NetImmerseNifDocument) -> Self {
        let mut lod = Self {
            controllers: BTreeMap::new(),
            nodes: BTreeMap::new(),
            shapes: BTreeMap::new(),
        };
        for block in document.blocks() {
            let NetImmerseNifBlockPayload::NiBoneLODController(controller) = &block.payload else {
                continue;
            };
            let initial = u16::try_from(controller.lod).unwrap_or(u16::MAX);
            let tier_count = controller.lod_count;
            if tier_count == 0 {
                continue;
            }
            lod.controllers.insert(
                block.index,
                NetImmerseBoneLodController {
                    target_ref: controller.controller.target_ref,
                    initial,
                    tier_count: u16::try_from(tier_count).unwrap_or(u16::MAX),
                },
            );
            for (ordinal, nodes) in controller.node_groups.iter().enumerate() {
                let Ok(ordinal) = u16::try_from(ordinal) else {
                    break;
                };
                for node in nodes {
                    lod.nodes.insert(
                        *node,
                        NetImmerseNodeBoneLod {
                            controller: block.index,
                            ordinal,
                            initial,
                        },
                    );
                }
            }
            for shape in &controller.shape_refs {
                lod.shapes.insert(
                    *shape,
                    NetImmerseShapeBoneLod {
                        controller: block.index,
                        initial,
                        skins: BTreeMap::new(),
                    },
                );
            }
            for (ordinal, shapes) in controller.shape_groups.iter().enumerate() {
                let Ok(ordinal) = u16::try_from(ordinal) else {
                    break;
                };
                for shape in shapes {
                    let plan = lod.shapes.entry(shape.shape_ref).or_insert_with(|| {
                        NetImmerseShapeBoneLod {
                            controller: block.index,
                            initial,
                            skins: BTreeMap::new(),
                        }
                    });
                    if plan.controller != block.index {
                        *plan = NetImmerseShapeBoneLod {
                            controller: block.index,
                            initial,
                            skins: BTreeMap::new(),
                        };
                    }
                    plan.skins.insert(ordinal, shape.skin_instance_ref);
                }
            }
        }
        lod
    }
}
