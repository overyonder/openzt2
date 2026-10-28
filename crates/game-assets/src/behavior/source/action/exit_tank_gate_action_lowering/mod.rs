use std::io;

use openzt2_game_data::behavior::action_record::BehaviorAction;

use super::invalid_behavior_source_data;
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

pub(super) fn lower_exit_tank_gate_action_source_node(
    source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if source_node.name != "ZTBehExitTankGate" {
        return Ok(None);
    }
    if !source_node.attributes.is_empty() || source_node.element_children().next().is_some() {
        return Err(invalid_behavior_source_data(
            "ZTBehExitTankGate has unsupported authored policy",
        ));
    }
    Ok(Some(BehaviorAction::ExitTankGate))
}
