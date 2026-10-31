use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use std::io;

pub(super) fn lower_spawn_object_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehSpawn" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        ![
            "spawnEntity",
            "subjectNode",
            "targetAnim",
            "randomPosOffset",
            "randomRotOffset",
        ]
        .contains(&attribute.name())
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehSpawn has unsupported spawn policy",
        ));
    }
    let required = |name| {
        read_optional_trimmed_string_attribute(node, name)
            .ok_or_else(|| invalid_behavior_source_data(format!("BFBehSpawn requires {name}")))
    };
    let offset = |name| -> io::Result<f32> {
        let value =
            super::source_value_reading::read_optional_float_attribute(node, name)?.unwrap_or(0.0);
        if !value.is_finite() || value < 0.0 {
            return Err(invalid_behavior_source_data(
                "spawn offsets must be finite and nonnegative",
            ));
        }
        Ok(value)
    };
    let entity = AssetId::from_key(&required("spawnEntity")?.to_ascii_lowercase());
    let pose = openzt2_game_data::behavior::action::spawn::BehaviorSpawnPosePolicy {
        random_position_offset_m: offset("randomPosOffset")?,
        random_rotation_offset_radians: offset("randomRotOffset")?,
    };
    if node.attribute("subjectNode").is_none() && node.attribute("targetAnim").is_none() {
        return Ok(Some(BehaviorAction::SpawnObjectAtActor { entity, pose }));
    }
    Ok(Some(BehaviorAction::SpawnObject {
        entity,
        subject_node: required("subjectNode")?,
        animation: required("targetAnim")?,
        pose,
    }))
}
