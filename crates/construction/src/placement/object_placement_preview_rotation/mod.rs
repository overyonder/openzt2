use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_data::world_definitions::object_placement::PlaceableDefinition;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;

use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;

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
    for mut preview in &mut previews {
        rotate_object_placement_transform_by_authored_steps(
            &mut preview.transform,
            definition,
            turns,
        );
    }
}

pub(super) fn rotate_object_placement_transform_by_authored_steps(
    transform: &mut Transform,
    definition: &PlaceableDefinition,
    steps: i32,
) {
    let increment = f32::from(definition.rotation_increment_degrees).to_radians();
    transform.rotate_y(increment * steps as f32);
}
