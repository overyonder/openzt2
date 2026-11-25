//! Morph data and the active controller on its geometry target.

use anyhow::{bail, ensure, Context};
use std::collections::BTreeSet;

use super::netimmerse_nif_source::{
    animation_controller_source_types::{
        NetImmerseMorph, NetImmerseNiMorphData, NetImmerseNiTimeController,
    },
    block_payload::NetImmerseNifBlockPayload,
    document_source_types::NetImmerseNifDocument,
};

pub(super) fn animated_morph_targets(data: &NetImmerseNiMorphData) -> &[NetImmerseMorph] {
    &data.morphs[usize::from(data.relative_targets != 0)..]
}

pub(super) fn geometry_morph_controller(
    document: &NetImmerseNifDocument,
    geometry: i32,
) -> anyhow::Result<Option<(&NetImmerseNiTimeController, &NetImmerseNiMorphData)>> {
    let object = document
        .block(geometry)
        .and_then(|block| block.payload.av_object())
        .context("invalid morph geometry")?;
    let mut reference = object.object.controller_ref;
    let mut visited = BTreeSet::new();
    let mut selected = None;
    while reference >= 0 {
        ensure!(
            visited.insert(reference),
            "cyclic geometry controller chain"
        );
        let payload = &document
            .block(reference)
            .context("missing geometry controller")?
            .payload;
        let Some(clock) = payload.time_controller() else {
            break;
        };
        if let NetImmerseNifBlockPayload::NiGeomMorpherController(controller) = payload {
            if clock.is_active() && clock.target_ref == geometry && controller.data_ref >= 0 {
                let Some(NetImmerseNifBlockPayload::NiMorphData(data)) = document
                    .block(controller.data_ref)
                    .map(|block| &block.payload)
                else {
                    bail!("morph controller does not reference NiMorphData");
                };
                if !data.morphs.is_empty() {
                    selected = Some((clock, data));
                }
            }
        }
        reference = clock.next_controller_ref;
    }
    Ok(selected)
}
