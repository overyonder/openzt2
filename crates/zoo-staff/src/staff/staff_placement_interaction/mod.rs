use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_zoo_state::construction::construction_interaction_types::{
    ConstructionCursor, ConstructionPreview, PlacementFailure, PlacementValidity,
};
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::{
    ConstructionTool, SelectConstructionTool,
};
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::zoo_cash_types::{UnlimitedZooCash, ZooCash};
use openzt2_zoo_state::placement::placement_preview_types::{
    ObjectPlacementPreviewPrefabHydrated, ObjectPlacementPreviewRequest,
};

use super::staff_lifecycle_messages::{BeginStaffHirePlacement, HireStaffRequest};
use super::staff_presentation_variant_selection::staff_role_for_definition;

#[derive(Component, PartialEq, Eq)]
pub(super) struct StaffPlacementRole(AssetId);

pub(super) fn begin_staff_placement_from_hire_action(
    mut commands: Commands,
    mut requests: MessageReader<BeginStaffHirePlacement>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut cursors: Query<
        (
            Entity,
            Option<&mut StaffPlacementRole>,
            Option<&mut ObjectPlacementPreviewRequest>,
        ),
        With<ConstructionCursor>,
    >,
    mut tools: MessageWriter<SelectConstructionTool>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(role) = requests
        .read()
        .filter_map(|request| staff_role_for_definition(catalogue, request.0))
        .last()
    else {
        return;
    };
    let Ok((entity, placement, preview)) = cursors.single_mut() else {
        return;
    };
    let mut cursor = commands.entity(entity);
    let selected_role = StaffPlacementRole(role.id);
    if let Some(mut placement) = placement {
        placement.set_if_neq(selected_role);
    } else {
        cursor.insert(selected_role);
    }
    let selected_preview = ObjectPlacementPreviewRequest(role.object);
    if let Some(mut preview) = preview {
        preview.set_if_neq(selected_preview);
    } else {
        cursor.insert(selected_preview);
    }
    tools.write(SelectConstructionTool(ConstructionTool::Placement));
}

pub(super) fn validate_staff_placement_preview_against_world_and_zoo_cash(
    zoo_cash: Single<&ZooCash>,
    unlimited_zoo_cash: Option<Single<&UnlimitedZooCash>>,
    cursors: Query<(&ConstructionCursor, &ObjectPlacementPreviewRequest), With<StaffPlacementRole>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut previews: Query<(
        &mut ConstructionPreview,
        Has<ObjectPlacementPreviewPrefabHydrated>,
    )>,
) {
    let (Ok((cursor, request)), Some(catalogue)) =
        (cursors.single(), active_definitions.get(&definitions))
    else {
        return;
    };
    let Some(object) = catalogue.find_object(request.0) else {
        return;
    };
    let cost = Money(object.price_cents);
    for (preview, hydrated) in &mut previews {
        if preview.definition != request.0 {
            continue;
        }
        let validity = if !cursor.over_terrain {
            PlacementValidity::Invalid(PlacementFailure::OutsideMap)
        } else if !hydrated {
            PlacementValidity::Pending
        } else if unlimited_zoo_cash.as_ref().is_none_or(|policy| !policy.0) && zoo_cash.0 < cost {
            PlacementValidity::Invalid(PlacementFailure::Unaffordable)
        } else {
            PlacementValidity::Valid { cost }
        };
        preview
            .map_unchanged(|preview| &mut preview.validity)
            .set_if_neq(validity);
    }
}

pub(super) fn request_staff_hiring_from_confirmed_placement(
    mut commands: Commands,
    mut pointer_inputs: MessageReader<PointerInput>,
    ui_pointer_capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    cursors: Query<
        (Entity, &StaffPlacementRole, &ObjectPlacementPreviewRequest),
        With<ConstructionCursor>,
    >,
    previews: Query<&ConstructionPreview>,
    mut hires: MessageWriter<HireStaffRequest>,
    mut tools: MessageWriter<SelectConstructionTool>,
) {
    let pressed = pointer_inputs.read().any(|input| {
        input.pointer_id.is_mouse() && input.button_just_pressed(PointerButton::Primary)
    });
    pointer_inputs.clear();
    if !pressed || *tool != ConstructionTool::Placement || ui_pointer_capture.over_ui {
        return;
    }
    let Ok((cursor, role, request)) = cursors.single() else {
        return;
    };
    let Some(preview) = previews.iter().find(|preview| {
        preview.definition == request.0
            && matches!(preview.validity, PlacementValidity::Valid { .. })
    }) else {
        return;
    };
    hires.write(HireStaffRequest {
        role: role.0,
        object: request.0,
        position: preview.transform.translation,
    });
    commands
        .entity(cursor)
        .remove::<(StaffPlacementRole, ObjectPlacementPreviewRequest)>();
    tools.write(SelectConstructionTool(ConstructionTool::Inspect));
}

pub(super) fn clear_staff_placement_after_leaving_placement_tool(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    cursors: Query<Entity, (With<ConstructionCursor>, With<StaffPlacementRole>)>,
) {
    if tool.is_changed() && *tool != ConstructionTool::Placement {
        for cursor in &cursors {
            commands
                .entity(cursor)
                .remove::<(StaffPlacementRole, ObjectPlacementPreviewRequest)>();
        }
    }
}
