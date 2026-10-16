use std::io;

use openzt2_game_data::behavior::action_record::BehaviorAction;

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::invalid_behavior_source_data;

pub mod animation_action_lowering;
pub mod attach_object_action_lowering;
pub mod docking_action_lowering;
pub mod economy_action_lowering;
pub mod entity_fact_modification_action_lowering;
pub mod feedback_action_lowering;
pub mod head_look_action_lowering;
pub mod hide_action_lowering;
pub mod interaction_container_action_lowering;
pub mod locomotion_switch_set_action_lowering;
pub mod move_action_lowering;
pub mod place_target_action_lowering;
pub mod play_set_action_lowering;
pub mod queue_action_lowering;
pub mod random_set_action_lowering;
pub mod script_action_lowering;
pub mod show_event_action_lowering;
pub mod source_value_reading;
pub mod spawn_object_action_lowering;
pub mod synchronized_set_action_lowering;
pub mod target_test_action_lowering;
pub mod termination_action_lowering;
pub mod token_action_lowering;
pub mod trick_learning_action_lowering;
pub mod view_animals_action_lowering;
pub mod view_event_action_lowering;
pub mod wander_action_lowering;
pub mod watch_show_action_lowering;

pub(super) fn lower_one_behavior_action_source_node(
    behavior_action_source_node: &DataNode,
) -> io::Result<BehaviorAction> {
    if let Some(action) =
        token_action_lowering::lower_self_token_delivery(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) = wander_action_lowering::lower_wander_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        show_event_action_lowering::lower_animal_show_event_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        watch_show_action_lowering::lower_watch_show_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        view_animals_action_lowering::lower_view_animals_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        trick_learning_action_lowering::lower_trick_learning_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        place_target_action_lowering::lower_place_target_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) = locomotion_switch_set_action_lowering::lower_locomotion_switch_set_action(
        behavior_action_source_node,
    )? {
        return Ok(action);
    }
    if let Some(action) = locomotion_switch_set_action_lowering::lower_animation_switch_set_action(
        behavior_action_source_node,
    )? {
        return Ok(action);
    }
    if let Some(action) =
        spawn_object_action_lowering::lower_spawn_object_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if behavior_action_source_node.name == "ZTBehCureDisease" {
        if !behavior_action_source_node.attributes.is_empty()
            || behavior_action_source_node
                .element_children()
                .next()
                .is_some()
        {
            return Err(invalid_behavior_source_data(
                "ZTBehCureDisease has unmapped authored policy",
            ));
        }
        return Ok(BehaviorAction::CureDisease);
    }
    if let Some(action) =
        attach_object_action_lowering::lower_attach_object_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) =
        attach_object_action_lowering::lower_detach_object_action(behavior_action_source_node)?
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
    if let Some(action) =
        queue_action_lowering::lower_queue_dock_action(behavior_action_source_node)?
    {
        return Ok(action);
    }
    if let Some(action) = hide_action_lowering::lower_hide_action(behavior_action_source_node)? {
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
    if let Some(action) =
        head_look_action_lowering::lower_head_look_action(behavior_action_source_node)?
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
