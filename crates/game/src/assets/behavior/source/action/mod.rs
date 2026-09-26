use std::io;

use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::invalid_behavior_source_data;

mod animation_action_lowering;
mod attach_object_action_lowering;
mod docking_action_lowering;
mod economy_action_lowering;
mod entity_fact_modification_action_lowering;
mod feedback_action_lowering;
mod interaction_container_action_lowering;
mod move_action_lowering;
mod play_set_action_lowering;
mod queue_action_lowering;
mod random_set_action_lowering;
mod script_action_lowering;
pub(super) mod source_value_reading;
mod synchronized_set_action_lowering;
mod target_test_action_lowering;
mod termination_action_lowering;
mod view_event_action_lowering;

pub(super) fn lower_one_behavior_action_source_node(
    behavior_action_source_node: &DataNode,
) -> io::Result<BehaviorAction> {
    if let Some(action) =
        attach_object_action_lowering::lower_attach_object_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        move_action_lowering::lower_move_action_source_node(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        synchronized_set_action_lowering::lower_synchronized_sets(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        queue_action_lowering::lower_queue_wait_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) = script_action_lowering::lower_script_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        economy_action_lowering::lower_economy_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        random_set_action_lowering::lower_random_set_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(lowered_behavior_action) =
        animation_action_lowering::lower_animation_action_source_node(behavior_action_source_node)?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        docking_action_lowering::lower_docking_action_source_node(behavior_action_source_node)?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        interaction_container_action_lowering::lower_interaction_container_action_source_node(
            behavior_action_source_node,
        )?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        play_set_action_lowering::lower_play_set_action_source_node(behavior_action_source_node)?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(action) =
        view_event_action_lowering::lower_view_event_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(lowered_behavior_action) =
        feedback_action_lowering::lower_feedback_action_source_node(behavior_action_source_node)?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        entity_fact_modification_action_lowering::lower_entity_fact_modification_action_source_node(
            behavior_action_source_node,
        )?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        target_test_action_lowering::lower_target_test_action_source_node(
            behavior_action_source_node,
        )?
    {
        return Ok(lowered_behavior_action);
    }
    if let Some(lowered_behavior_action) =
        termination_action_lowering::lower_termination_action_source_node(
            behavior_action_source_node,
        )?
    {
        return Ok(lowered_behavior_action);
    }

    Err(invalid_behavior_source_data(format!(
        "unmapped behavior action {}",
        behavior_action_source_node.name
    )))
}
