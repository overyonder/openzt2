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
    if node.attribute("spawnRelation") == Some("child") {
        return lower_spawn_child_action(node);
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
    Ok(Some(BehaviorAction::SpawnObject {
        entity: AssetId::from_key(&required("spawnEntity")?.to_ascii_lowercase()),
        subject_node: required("subjectNode")?,
        animation: required("targetAnim")?,
        pose: openzt2_game_data::behavior::action::spawn::BehaviorSpawnPosePolicy {
            random_position_offset_m: offset("randomPosOffset")?,
            random_rotation_offset_radians: offset("randomRotOffset")?,
        },
    }))
}

/// A birth. `subjectNode="Floor"` places the child at the mother's feet.
/// ponytail: `fadeInPeriod` and `spawnInTA` (fade-in, placement inside the
/// habitat) are not applied; the child appears at once beside the mother.
fn lower_spawn_child_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.attributes.iter().any(|attribute| {
        ![
            "spawnEntity",
            "spawnRelation",
            "subjectNode",
            "fadeInPeriod",
            "randomPosOffset",
            "randomRotOffset",
            "spawnInTA",
        ]
        .contains(&attribute.name())
    }) || node.element_children().next().is_some()
        || node
            .attribute("subjectNode")
            .is_some_and(|node| !node.eq_ignore_ascii_case("floor"))
    {
        return Err(invalid_behavior_source_data(
            "BFBehSpawn child has unsupported spawn policy",
        ));
    }
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
    let entity = read_optional_trimmed_string_attribute(node, "spawnEntity")
        .ok_or_else(|| invalid_behavior_source_data("BFBehSpawn requires spawnEntity"))?;
    Ok(Some(BehaviorAction::SpawnChild {
        entity: AssetId::from_key(&entity.to_ascii_lowercase()),
        pose: openzt2_game_data::behavior::action::spawn::BehaviorSpawnPosePolicy {
            random_position_offset_m: offset("randomPosOffset")?,
            random_rotation_offset_radians: offset("randomRotOffset")?,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn birth_spawn_names_the_young_variant() {
        let document = parse_blue_fang_source_document(AssetPath::new("aardvark.beh"),
            br#"<behaviors><BFBehSpawn spawnEntity="Aardvark_Young_F" spawnRelation="child" subjectNode="Floor" fadeInPeriod="1" randomPosOffset="3" randomRotOffset="180" spawnInTA="true"/></behaviors>"#).expect("source");
        let Some(BehaviorAction::SpawnChild { entity, pose }) =
            lower_spawn_object_action(document.root.element_children().next().expect("action"))
                .expect("lower")
        else {
            panic!("child spawn")
        };
        assert_eq!(entity, AssetId::from_key("aardvark_young_f"));
        assert_eq!(pose.random_position_offset_m, 3.0);
    }
}
