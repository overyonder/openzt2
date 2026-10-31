//! Assembly of behavior-set and behavior-task documents from repaired source nodes.

use std::io;

use openzt2_game_data::{
    behavior::document::{BehaviorDocument, BehaviorSet, BehaviorTask},
    AssetId,
};

use crate::source_document::ordered_source_document_types::{
    OrderedSourceDocument as DataDocument, OrderedSourceDocumentNode as DataNode,
};

pub mod action;
pub mod eligibility;
pub mod scalar;
pub mod scoring;

#[cfg(test)]
mod tests;

pub(super) fn lower_behavior_source_document(
    source_document: &DataDocument,
) -> io::Result<BehaviorDocument> {
    match source_document.root.name.as_str() {
        "BehaviorSets" => source_document
            .root
            .element_children()
            .map(|source_node| lower_behavior_set_declaration(source_document, source_node))
            .collect::<io::Result<Vec<_>>>()
            .map(BehaviorDocument::Sets),
        "BFAITaskTemplateList" => source_document
            .root
            .element_children()
            .filter(|source_node| source_node.name == "BFAITaskTemplate")
            .map(|source_node| lower_behavior_task_declaration(source_document, source_node))
            .collect::<io::Result<Vec<_>>>()
            .map(BehaviorDocument::Tasks),
        unsupported_root_name => Err(invalid_behavior_source_data(format!(
            "unsupported behavior document root {unsupported_root_name}"
        ))),
    }
}

fn lower_behavior_set_declaration(
    source_document: &DataDocument,
    behavior_set_node: &DataNode,
) -> io::Result<BehaviorSet> {
    let subject_type_names = find_direct_child_behavior_source_node(behavior_set_node, "subjects")
        .map_or_else(Vec::new, collect_direct_child_names);
    let behavior_set_name = behavior_set_node.name.to_string();
    Ok(BehaviorSet {
        id: create_behavior_declaration_identifier(
            source_document,
            &behavior_set_name,
            &subject_type_names,
        ),
        name: behavior_set_name,
        subjects: subject_type_names,
        actions: lower_behavior_action_phase(find_direct_child_behavior_source_node(
            behavior_set_node,
            "behaviors",
        ))?,
    })
}

fn lower_behavior_task_declaration(
    source_document: &DataDocument,
    behavior_task_node: &DataNode,
) -> io::Result<BehaviorTask> {
    let task_creation_node =
        find_direct_child_behavior_source_node(behavior_task_node, "BFAICreateData");
    let subject_type_names = task_creation_node
        .and_then(|source_node| {
            find_direct_child_behavior_source_node_starting_with(source_node, "Subjects")
        })
        .map_or_else(Vec::new, collect_behavior_participant_type_names);
    let target_type_names = task_creation_node
        .and_then(|source_node| {
            find_direct_child_behavior_source_node_starting_with(source_node, "Targets")
        })
        .map_or_else(Vec::new, collect_behavior_participant_type_names);
    let object_type_names = task_creation_node
        .and_then(|source_node| {
            find_direct_child_behavior_source_node_starting_with(source_node, "Objects")
        })
        .map_or_else(Vec::new, collect_behavior_participant_type_names);
    let behavior_task_name = behavior_task_node
        .attribute("UniqueID")
        .or_else(|| behavior_task_node.attribute("Name"))
        .unwrap_or("<anonymous>")
        .to_owned();
    Ok(BehaviorTask {
        id: create_behavior_declaration_identifier(
            source_document,
            &behavior_task_name,
            &subject_type_names,
        ),
        name: behavior_task_name,
        priority: behavior_task_node
            .attribute("Priority")
            .map(|priority| {
                priority
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        invalid_behavior_source_data(format!(
                            "invalid behavior task Priority {priority:?}"
                        ))
                    })
            })
            .transpose()?,
        task_delay_seconds: lower_behavior_task_delay(behavior_task_node)?,
        reservation_tag: behavior_task_node
            .attribute("reserveTag")
            .map(str::trim)
            .filter(|reservation_tag| !reservation_tag.is_empty())
            .map(|reservation_tag| AssetId::from_key(&reservation_tag.to_ascii_lowercase())),
        sensor_tag: behavior_task_node
            .attribute("sensorTag")
            .map(str::trim)
            .filter(|sensor_tag| !sensor_tag.is_empty())
            .map(|sensor_tag| AssetId::from_key(&sensor_tag.to_ascii_lowercase())),
        subjects: subject_type_names,
        targets: target_type_names,
        objects: object_type_names,
        candidate_eligibility_requirements:
            eligibility::lower_behavior_task_candidate_eligibility_requirements(behavior_task_node)?,
        scores: find_direct_child_behavior_source_node(behavior_task_node, "BFAIEvalData")
            .map(scoring::lower_behavior_evaluation)
            .transpose()?
            .unwrap_or_default(),
        execution: lower_behavior_action_phase(find_direct_child_behavior_source_node(
            behavior_task_node,
            "BFBehExecTask",
        ))?,
        completion: lower_behavior_outcome_phase(find_direct_child_behavior_source_node(
            behavior_task_node,
            "BFAICompletionData",
        ))?,
        failure: lower_behavior_outcome_phase(find_direct_child_behavior_source_node(
            behavior_task_node,
            "BFAIFailureData",
        ))?,
    })
}

fn lower_behavior_task_delay(node: &DataNode) -> io::Result<Option<[f32; 2]>> {
    let minimum = node.attribute("TaskDelayMin");
    let maximum = node.attribute("TaskDelayMax");
    match (minimum, maximum) {
        (None, None) => Ok(None),
        (minimum, maximum) => {
            let parse = |value: &str| -> io::Result<f32> {
                value
                    .parse()
                    .map_err(|_| invalid_behavior_source_data("invalid task delay"))
            };
            let range = [
                parse(minimum.unwrap_or("0"))?,
                parse(maximum.unwrap_or("0"))?,
            ];
            if range.iter().all(|value| value.is_finite() && *value >= 0.0) && range[0] <= range[1]
            {
                Ok(Some(range))
            } else {
                Err(invalid_behavior_source_data("invalid task delay range"))
            }
        }
    }
}

fn lower_behavior_outcome_phase(
    outcome: Option<&DataNode>,
) -> io::Result<openzt2_game_data::behavior::document::BehaviorTaskActionPhase> {
    use openzt2_game_data::behavior::{
        action_record::BehaviorAction, document::BehaviorTaskActionPhase,
    };

    let Some(outcome) = outcome else {
        return lower_behavior_action_phase(None);
    };
    // Terminal subject and target effects belong to the same outcome, before
    // its continuation. Validate them together so an unavailable target cannot
    // leave an animal with the need benefit but no corresponding quantity write.
    let need_map = find_direct_child_behavior_source_node(outcome, "BFAIAttributeFloatMap");
    let terminal_nodes: Vec<_> = need_map
        .into_iter()
        .chain(outcome.element_children().filter(|node| {
            !matches!(
                node.name.as_str(),
                "BFAIAttributeFloatMap" | "BFBehExecTask"
            )
        }))
        .collect();
    let mut phase = lower_behavior_action_nodes(
        terminal_nodes.iter().copied().chain(
            find_direct_child_behavior_source_node(outcome, "BFBehExecTask")
                .into_iter()
                .flat_map(DataNode::element_children),
        ),
    )?;
    if let BehaviorTaskActionPhase::Supported(actions)
    | BehaviorTaskActionPhase::SupportedPrefix { actions, .. } = &mut phase
    {
        let terminal_fact_count = actions
            .iter()
            .take(terminal_nodes.len())
            .take_while(|action| matches!(action, BehaviorAction::FactModifications(_)))
            .count();
        if terminal_fact_count > 1 {
            let mut modifications = Vec::new();
            for action in actions.drain(..terminal_fact_count) {
                if let BehaviorAction::FactModifications(facts) = action {
                    modifications.extend(facts);
                }
            }
            actions.insert(0, BehaviorAction::FactModifications(modifications));
        }
    }
    Ok(phase)
}

fn lower_behavior_action_phase(
    action_container_node: Option<&DataNode>,
) -> io::Result<openzt2_game_data::behavior::document::BehaviorTaskActionPhase> {
    use openzt2_game_data::behavior::document::BehaviorTaskActionPhase;

    let Some(action_container_node) = action_container_node else {
        return Ok(BehaviorTaskActionPhase::Supported(Vec::new()));
    };
    lower_behavior_action_nodes(action_container_node.element_children())
}

fn lower_behavior_action_nodes<'a>(
    nodes: impl Iterator<Item = &'a DataNode>,
) -> io::Result<openzt2_game_data::behavior::document::BehaviorTaskActionPhase> {
    use openzt2_game_data::behavior::document::BehaviorTaskActionPhase;

    let mut lowered_actions = Vec::new();
    for action_node in nodes {
        // An animated detach lowers to its clip and then the detach itself.
        let lowered =
            match action::attach_object_action_lowering::lower_animated_detach_object_actions(
                action_node,
            ) {
                Ok(Some(actions)) => Ok(actions.to_vec()),
                Ok(None) => action::lower_one_behavior_action_source_node(action_node)
                    .map(|action| vec![action]),
                Err(error) => Err(error),
            };
        match lowered {
            Ok(actions) => lowered_actions.extend(actions),
            Err(_) if lowered_actions.is_empty() => {
                return Ok(BehaviorTaskActionPhase::Unsupported {
                    first_unsupported_action: action_node.name.to_string(),
                });
            }
            Err(_) => {
                return Ok(BehaviorTaskActionPhase::SupportedPrefix {
                    actions: lowered_actions,
                    first_unsupported_action: action_node.name.to_string(),
                });
            }
        }
    }
    Ok(BehaviorTaskActionPhase::Supported(lowered_actions))
}

fn create_behavior_declaration_identifier(
    source_document: &DataDocument,
    declaration_name: &str,
    subject_type_names: &[String],
) -> AssetId {
    AssetId::from_key(&format!(
        "{}#{}@{}",
        source_document.path.key(),
        declaration_name.trim().to_ascii_lowercase(),
        format_normalized_behavior_subject_signature(subject_type_names)
    ))
}

fn format_normalized_behavior_subject_signature(subject_type_names: &[String]) -> String {
    let mut normalized_subject_type_names = subject_type_names
        .iter()
        .map(|subject_type_name| subject_type_name.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    normalized_subject_type_names.sort_unstable();
    normalized_subject_type_names.dedup();
    normalized_subject_type_names.join("+")
}

fn find_direct_child_behavior_source_node<'a>(
    parent_source_node: &'a DataNode,
    child_node_name: &str,
) -> Option<&'a DataNode> {
    parent_source_node
        .element_children()
        .find(|child_source_node| child_source_node.name == child_node_name)
}

fn find_direct_child_behavior_source_node_starting_with<'a>(
    parent_source_node: &'a DataNode,
    child_node_name_prefix: &str,
) -> Option<&'a DataNode> {
    parent_source_node
        .element_children()
        .find(|child_source_node| child_source_node.name.starts_with(child_node_name_prefix))
}

fn collect_direct_child_names(parent_source_node: &DataNode) -> Vec<String> {
    parent_source_node
        .element_children()
        .map(|child_source_node| child_source_node.name.trim().to_ascii_lowercase())
        .collect()
}

fn collect_behavior_participant_type_names(participant_container_node: &DataNode) -> Vec<String> {
    participant_container_node
        .element_children()
        .filter(|child_source_node| !child_source_node.name.starts_with("Qualifiers"))
        .map(|child_source_node| child_source_node.name.trim().to_ascii_lowercase())
        .collect()
}

fn invalid_behavior_source_data(diagnostic_message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, diagnostic_message.into())
}
