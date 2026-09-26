use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::input::input_types::ActionRequest;
use crate::plugins::input::input_types::GameAction;

use super::object_placement_definition_queries::resolve_object_placeable_definition;

/// Camera rotation yields to placement while a preview is active.
pub(super) fn rotate_active_object_placement_preview_by_authored_increment(
    mut actions: MessageReader<ActionRequest>,
    tool: Res<ConstructionTool>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut previews: Query<&mut ConstructionPreview>,
) {
    let ConstructionTool::Place(definition_id) = *tool else {
        return;
    };
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(definition) = resolve_object_placeable_definition(catalogue, definition_id) else {
        return;
    };
    let increment = definition.rotation_increment_degrees;
    if increment == 0 {
        return;
    }
    let turns = actions.read().fold(0_i32, |turns, request| {
        turns
            + match request.action {
                GameAction::RotateLeft | GameAction::RotateObjectLeft => -1,
                GameAction::RotateRight | GameAction::RotateObjectRight => 1,
                _ => 0,
            }
    });
    if turns == 0 {
        return;
    }
    let radians = f32::from(increment).to_radians() * turns as f32;
    for mut preview in &mut previews {
        preview.transform.rotate_y(radians);
    }
}
