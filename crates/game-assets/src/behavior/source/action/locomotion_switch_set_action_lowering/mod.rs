use std::io;

use openzt2_game_data::{
    behavior::{
        action::locomotion_switch_set::{
            BehaviorAnimationSwitchSetAction, BehaviorLocomotionSwitchSetAction,
        },
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};

/// Carrying variants, which select by held items, have no runtime owner and
/// are rejected through their unmapped medium rows.
pub(super) fn lower_locomotion_switch_set_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehLocoSwitchSet" {
        return Ok(None);
    }
    let follows_medium_changes = match node.attribute("switch") {
        None => true,
        Some(value) if value.trim().eq_ignore_ascii_case("false") => false,
        Some(value) if value.trim().eq_ignore_ascii_case("true") => true,
        Some(_) => {
            return Err(invalid_behavior_source_data(
                "BFBehLocoSwitchSet has an invalid switch value",
            ))
        }
    };
    if node
        .attributes
        .iter()
        .any(|attribute| attribute.name() != "switch")
        || node
            .element_children()
            .any(|child| child.name != "behaviorTable")
    {
        return Err(invalid_behavior_source_data(
            "BFBehLocoSwitchSet has unmapped switching policy",
        ));
    }
    let mut action = BehaviorLocomotionSwitchSetAction {
        ground: None,
        water: None,
        underwater: None,
        follows_medium_changes,
    };
    for entry in node
        .element_children()
        .flat_map(OrderedSourceDocumentNode::element_children)
    {
        let slot = match entry.name.as_str() {
            "ground" => &mut action.ground,
            "water" => &mut action.water,
            "underwater" => &mut action.underwater,
            _ => {
                return Err(invalid_behavior_source_data(
                    "BFBehLocoSwitchSet has an unmapped locomotion medium",
                ))
            }
        };
        if slot.is_some()
            || entry
                .attributes
                .iter()
                .any(|attribute| attribute.name() != "behSet")
            || entry.element_children().next().is_some()
        {
            return Err(invalid_behavior_source_data(
                "BFBehLocoSwitchSet medium has unmapped policy",
            ));
        }
        *slot = Some(AssetId::from_key(
            &read_optional_trimmed_string_attribute(entry, "behSet")
                .ok_or_else(|| {
                    invalid_behavior_source_data("BFBehLocoSwitchSet medium has no behSet")
                })?
                .to_ascii_lowercase(),
        ));
    }
    Ok(Some(BehaviorAction::LocomotionSwitchSet(action)))
}

/// Only the form that selects once is mapped; `loopFlag` replays the
/// selection indefinitely and has no runtime owner.
pub(super) fn lower_animation_switch_set_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehAnimSwitchSet" {
        return Ok(None);
    }
    if !node.attributes.is_empty()
        || node
            .element_children()
            .any(|child| child.name != "behaviorTable")
    {
        return Err(invalid_behavior_source_data(
            "BFBehAnimSwitchSet has unmapped switching policy",
        ));
    }
    let mut action = BehaviorAnimationSwitchSetAction {
        sets_by_animation_graph_node: Vec::new(),
        default_set: None,
    };
    for entry in node
        .element_children()
        .flat_map(OrderedSourceDocumentNode::element_children)
    {
        if entry
            .attributes
            .iter()
            .any(|attribute| attribute.name() != "behSet")
            || entry.element_children().next().is_some()
        {
            return Err(invalid_behavior_source_data(
                "BFBehAnimSwitchSet row has unmapped policy",
            ));
        }
        let set = AssetId::from_key(
            &read_optional_trimmed_string_attribute(entry, "behSet")
                .ok_or_else(|| {
                    invalid_behavior_source_data("BFBehAnimSwitchSet row has no behSet")
                })?
                .to_ascii_lowercase(),
        );
        if entry.name.eq_ignore_ascii_case("default") {
            if action.default_set.replace(set).is_some() {
                return Err(invalid_behavior_source_data(
                    "BFBehAnimSwitchSet has two default rows",
                ));
            }
        } else {
            action
                .sets_by_animation_graph_node
                .push((entry.name.to_string(), set));
        }
    }
    Ok(Some(BehaviorAction::AnimationSwitchSet(action)))
}
