use std::io;

use openzt2_game_data::behavior::{
    action::termination::BehaviorTerminationAction, action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{invalid_behavior_source_data, source_value_reading::read_boolean_attribute_or_false};

pub(super) fn lower_termination_action_source_node(
    termination_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if termination_action_source_node.name != "BFBehKill" {
        return Ok(None);
    }
    if termination_action_source_node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "killSubject" | "killTarget"))
        || termination_action_source_node
            .element_children()
            .next()
            .is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehKill has unmapped authored policy",
        ));
    }
    let termination_action = BehaviorTerminationAction {
        kill_subject: read_boolean_attribute_or_false(
            termination_action_source_node,
            "killSubject",
        )?,
        kill_target: read_boolean_attribute_or_false(termination_action_source_node, "killTarget")?,
    };
    if !termination_action.kill_subject && !termination_action.kill_target {
        return Err(invalid_behavior_source_data(
            "BFBehKill does not identify a subject or target",
        ));
    }
    Ok(Some(BehaviorAction::Termination(termination_action)))
}
