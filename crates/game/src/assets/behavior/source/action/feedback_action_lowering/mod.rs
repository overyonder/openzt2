use std::io;

use openzt2_game_data::behavior::{
    action::feedback::{BehaviorFeedbackAction, BehaviorFeedbackEntry},
    action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_float_attribute,
        read_optional_trimmed_string_attribute,
    },
};

pub(super) fn lower_feedback_action_source_node(
    feedback_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if feedback_action_source_node.name != "ZTBehFeedback" {
        return Ok(None);
    }
    if !feedback_action_source_node.attributes.is_empty() {
        return Err(invalid_behavior_source_data(
            "ZTBehFeedback has unmapped authored policy",
        ));
    }
    let mut feedback_data_nodes = feedback_action_source_node.element_children();
    let feedback_data_node = feedback_data_nodes
        .next()
        .filter(|source_node| source_node.name == "ZTFeedbackData")
        .ok_or_else(|| invalid_behavior_source_data("ZTBehFeedback has no ZTFeedbackData"))?;
    if feedback_data_nodes.next().is_some() || !feedback_data_node.attributes.is_empty() {
        return Err(invalid_behavior_source_data(
            "ZTBehFeedback has unmapped feedback data",
        ));
    }
    Ok(Some(BehaviorAction::Feedback(lower_feedback_data(
        feedback_data_node,
    )?)))
}

pub(super) fn lower_feedback_data(
    feedback_data_node: &DataNode,
) -> io::Result<BehaviorFeedbackAction> {
    if feedback_data_node.name != "ZTFeedbackData" || !feedback_data_node.attributes.is_empty() {
        return Err(invalid_behavior_source_data("unmapped feedback data"));
    }
    let entries = feedback_data_node
        .element_children()
        .map(lower_feedback_entry)
        .collect::<io::Result<Vec<_>>>()?;
    if entries.is_empty() {
        return Err(invalid_behavior_source_data(
            "ZTFeedbackData has no feedback entries",
        ));
    }
    Ok(BehaviorFeedbackAction { entries })
}

fn lower_feedback_entry(
    feedback_entry_source_node: &DataNode,
) -> io::Result<BehaviorFeedbackEntry> {
    if feedback_entry_source_node
        .element_children()
        .next()
        .is_some()
    {
        return Err(invalid_behavior_source_data(format!(
            "{} has unmapped nested feedback data",
            feedback_entry_source_node.name
        )));
    }
    match feedback_entry_source_node.name.as_str() {
        "ZTActionInfo" => lower_action_feedback_entry(feedback_entry_source_node),
        "ZTThoughtInfo" => lower_thought_feedback_entry(feedback_entry_source_node),
        "ZTMessageInfo" => lower_message_feedback_entry(feedback_entry_source_node),
        "ZTEmoticonInfo" => lower_emoticon_feedback_entry(feedback_entry_source_node),
        unsupported_feedback_entry => Err(invalid_behavior_source_data(format!(
            "unsupported feedback entry {unsupported_feedback_entry}"
        ))),
    }
}

fn lower_action_feedback_entry(source_node: &DataNode) -> io::Result<BehaviorFeedbackEntry> {
    require_only_attributes(source_node, &["locID", "useEntityName", "useTargetName"])?;
    Ok(BehaviorFeedbackEntry::Action {
        localization_key: require_trimmed_string_attribute(source_node, "locID")?,
        use_entity_name: read_boolean_attribute_or_false(source_node, "useEntityName")?,
        use_target_name: read_boolean_attribute_or_false(source_node, "useTargetName")?,
    })
}

fn lower_thought_feedback_entry(source_node: &DataNode) -> io::Result<BehaviorFeedbackEntry> {
    require_only_attributes(
        source_node,
        &[
            "locID",
            "priority",
            "timeout",
            "global",
            "useEntityName",
            "useTargetName",
        ],
    )?;
    Ok(BehaviorFeedbackEntry::Thought {
        localization_key: require_trimmed_string_attribute(source_node, "locID")?,
        priority: read_optional_integer_attribute(source_node, "priority")?,
        timeout_seconds: read_optional_nonnegative_integer_attribute(source_node, "timeout")?,
        globally_visible: read_optional_boolean_attribute(source_node, "global")?,
        use_entity_name: read_boolean_attribute_or_false(source_node, "useEntityName")?,
        use_target_name: read_boolean_attribute_or_false(source_node, "useTargetName")?,
    })
}

fn lower_message_feedback_entry(source_node: &DataNode) -> io::Result<BehaviorFeedbackEntry> {
    require_only_attributes(
        source_node,
        &[
            "locID",
            "priority",
            "timeout",
            "interval",
            "tolerance",
            "global",
            "useEntityName",
            "useTargetName",
            "filterAttribute",
        ],
    )?;
    Ok(BehaviorFeedbackEntry::Message {
        localization_key: require_trimmed_string_attribute(source_node, "locID")?,
        priority: read_optional_integer_attribute(source_node, "priority")?,
        timeout_seconds: read_optional_nonnegative_integer_attribute(source_node, "timeout")?,
        interval_seconds: read_optional_nonnegative_integer_attribute(source_node, "interval")?,
        tolerance: read_optional_float_attribute(source_node, "tolerance")?,
        globally_visible: read_optional_boolean_attribute(source_node, "global")?,
        use_entity_name: read_boolean_attribute_or_false(source_node, "useEntityName")?,
        use_target_name: read_boolean_attribute_or_false(source_node, "useTargetName")?,
        filter_attribute_expression: read_optional_trimmed_string_attribute(
            source_node,
            "filterAttribute",
        ),
    })
}

fn lower_emoticon_feedback_entry(source_node: &DataNode) -> io::Result<BehaviorFeedbackEntry> {
    require_only_attributes(source_node, &["emoticonName"])?;
    Ok(BehaviorFeedbackEntry::Emoticon {
        emoticon_name: require_trimmed_string_attribute(source_node, "emoticonName")?,
    })
}

fn require_only_attributes(source_node: &DataNode, names: &[&str]) -> io::Result<()> {
    if source_node
        .attributes
        .iter()
        .all(|attribute| names.contains(&attribute.name()))
    {
        Ok(())
    } else {
        Err(invalid_behavior_source_data(format!(
            "{} has unmapped authored policy",
            source_node.name
        )))
    }
}

fn require_trimmed_string_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<String> {
    read_optional_trimmed_string_attribute(source_node, attribute_name).ok_or_else(|| {
        invalid_behavior_source_data(format!("{} has no {attribute_name}", source_node.name))
    })
}

fn read_optional_integer_attribute<T>(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<Option<T>>
where
    T: std::str::FromStr,
{
    source_node
        .attribute(attribute_name)
        .map(|authored_integer| {
            authored_integer.parse().map_err(|_| {
                invalid_behavior_source_data(format!(
                    "{} has invalid {attribute_name}",
                    source_node.name
                ))
            })
        })
        .transpose()
}

fn read_optional_nonnegative_integer_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<Option<u16>> {
    read_optional_integer_attribute(source_node, attribute_name)
}

fn read_optional_boolean_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<Option<bool>> {
    source_node
        .attribute(attribute_name)
        .map(|_| read_boolean_attribute_or_false(source_node, attribute_name))
        .transpose()
}
