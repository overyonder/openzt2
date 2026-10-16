use super::{
    invalid_behavior_source_data, source_value_reading::read_optional_trimmed_string_attribute,
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{
    behavior::{
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
    if node.name != "BFBehDetachObject" {
        return Ok(None);
    }
    // A detach animation, a container or a node changes when and where the
    // item goes; only the immediate detach of a named item is mapped.
    if node
        .attributes
        .iter()
        .any(|attribute| !["detachEntity", "detachAction"].contains(&attribute.name()))
        || node.element_children().next().is_some()
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
    Ok(Some(BehaviorAction::DetachObject(
        BehaviorDetachObjectAction {
            entity: identifier("detachEntity")?,
            detach_rule: identifier("detachAction")?,
        },
    )))
}
