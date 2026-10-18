use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{
    behavior::{
        action::animation::BehaviorAnimationClipAction,
        action::attach_object::{BehaviorAttachObjectAction, BehaviorDetachObjectAction},
        action_record::BehaviorAction,
    },
    AssetId,
};
use std::io;

pub(super) fn lower_attach_object_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehAttachObject" {
        return Ok(None);
    }
    // Extra attachment policy has no runtime support; ignoring it changes the action.
    if node.attributes.iter().any(|attribute| {
        ![
            "attachEntity",
            "targetAnim",
            "container",
            "detachAction",
            "locoMod",
            "subjectNode",
            "targetNode",
            "detachBehSet",
        ]
        .contains(&attribute.name())
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehAttachObject has unsupported attachment policy",
        ));
    }
    let entity = read_optional_trimmed_string_attribute(node, "attachEntity")
        .ok_or_else(|| invalid_behavior_source_data("BFBehAttachObject requires attachEntity"))?;
    let target_node = read_optional_trimmed_string_attribute(node, "targetNode");
    // A target node only places the target container's held prop.
    if (target_node.is_some() || node.attribute("detachBehSet").is_some())
        && !entity.eq_ignore_ascii_case("s_prop")
    {
        return Err(invalid_behavior_source_data(
            "BFBehAttachObject target node is only mapped for s_prop",
        ));
    }
    let identifier = |name| {
        read_optional_trimmed_string_attribute(node, name)
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
            .unwrap_or_default()
    };
    Ok(Some(BehaviorAction::AttachObject(
        BehaviorAttachObjectAction {
            entity: AssetId::from_key(&entity.to_ascii_lowercase()),
            animation: read_optional_trimmed_string_attribute(node, "targetAnim"),
            container: identifier("container"),
            detach_rule: identifier("detachAction"),
            locomotion_modifier: read_optional_trimmed_string_attribute(node, "locoMod")
                .map(|value| AssetId::from_key(&value.to_ascii_lowercase())),
            subject_node: read_optional_trimmed_string_attribute(node, "subjectNode"),
            target_node,
            detach_behavior_set: read_optional_trimmed_string_attribute(node, "detachBehSet")
                .map(|value| AssetId::from_key(&value.to_ascii_lowercase())),
        },
    )))
}

pub(super) fn lower_detach_object_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehDetachObject" || node.attribute("targetAnim").is_some() {
        return Ok(None);
    }
    lower_detach_object_rule(node).map(Some)
}

/// `BFBehDetachObject` with a `targetAnim`, such as a guest's
/// `StandItem_ThrowAway`: the clip plays, then the rule applies. A detach key
/// in the clip only returns the item to the holder's inventory first.
pub(crate) fn lower_animated_detach_object_actions(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<[BehaviorAction; 2]>> {
    if node.name != "BFBehDetachObject" {
        return Ok(None);
    }
    let Some(animation_clip_asset_key) = node
        .attribute("targetAnim")
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    Ok(Some([
        BehaviorAction::AnimationClip(BehaviorAnimationClipAction {
            animation_clip_asset_key,
            looping: false,
            physical_object: None,
            play_duration_seconds: None,
            interrupts_current_animation: false,
            interruptible: false,
            ground_fit: None,
            blend_duration_seconds: None,
            playback_speed: None,
        }),
        lower_detach_object_rule(node)?,
    ]))
}

fn lower_detach_object_rule(node: &OrderedSourceDocumentNode) -> io::Result<BehaviorAction> {
    // A container or a node changes where the item goes; only the detach of a
    // named item, optionally after its clip, is mapped.
    if node.attributes.iter().any(|attribute| {
        !["detachEntity", "detachAction", "targetAnim"].contains(&attribute.name())
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehDetachObject has unsupported detach policy",
        ));
    }
    let identifier = |name| {
        read_optional_trimmed_string_attribute(node, name)
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
            .ok_or_else(|| {
                invalid_behavior_source_data(format!("BFBehDetachObject requires {name}"))
            })
    };
    Ok(BehaviorAction::DetachObject(BehaviorDetachObjectAction {
        entity: identifier("detachEntity")?,
        detach_rule: identifier("detachAction")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn animated_detach_plays_its_clip_before_the_rule() {
        // Trash.beh UseTrashContainer.
        let source = parse_blue_fang_source_document(
            AssetPath::new("trash.beh"),
            br#"<behaviors><BFBehDetachObject detachEntity="object" targetAnim="StandItem_ThrowAway" detachAction="killitem"/></behaviors>"#,
        )
        .expect("source");
        let node = source.root.element_children().next().expect("action");
        assert!(lower_detach_object_action(node).expect("lower").is_none());
        let [BehaviorAction::AnimationClip(clip), BehaviorAction::DetachObject(detach)] =
            lower_animated_detach_object_actions(node)
                .expect("lower")
                .expect("animated detach")
        else {
            panic!("clip then detach")
        };
        assert_eq!(clip.animation_clip_asset_key, "StandItem_ThrowAway");
        assert_eq!(detach.entity, AssetId::from_key("object"));
        assert_eq!(detach.detach_rule, AssetId::from_key("killitem"));
    }
}
