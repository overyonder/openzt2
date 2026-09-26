use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionCursor;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::construction::construction_tool_and_placement_policy_types::SelectConstructionTool;
use crate::plugins::economy::money_types::Money;
use crate::plugins::economy::zoo_cash_types::UnlimitedZooCash;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::information::catalogue_types::PurchaseChoice;
use crate::plugins::placement::placement_preview_types::ObjectPlacementPreviewPrefabHydrated;
use crate::plugins::placement::placement_preview_types::ObjectPlacementPreviewRequest;
use crate::plugins::ui::picking::UiPointerCapture;

use super::staff_lifecycle_messages::HireStaffRequest;

#[derive(Component, Debug, Clone, Copy)]
pub(in crate::plugins::staff) struct PendingStaffPlacement {
    staff_role: openzt2_game_data::AssetId,
}

pub(in crate::plugins::staff) fn validate_staff_placement_preview_against_world_and_zoo_cash(
    zoo_cash: Res<ZooCash>,
    unlimited_zoo_cash: Option<Res<UnlimitedZooCash>>,
    cursors: Query<(
        &ConstructionCursor,
        &PendingStaffPlacement,
        &ObjectPlacementPreviewRequest,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    hydrated_previews: Query<(), With<ObjectPlacementPreviewPrefabHydrated>>,
    mut previews: Query<(Entity, &mut ConstructionPreview)>,
) {
    let (Ok((cursor, pending_placement, requested_prefab)), Some(definitions)) =
        (cursors.single(), active_definitions.get(&definitions))
    else {
        return;
    };
    let Some(staff_role) = definitions.find_staff(pending_placement.staff_role) else {
        return;
    };
    let Some(staff_object) =
        definitions.find_object(openzt2_game_data::AssetId(staff_role.object.0))
    else {
        return;
    };
    for (preview_entity, mut preview) in &mut previews {
        if preview.definition != requested_prefab.0 {
            continue;
        }
        let next_validity = if !cursor.over_terrain {
            PlacementValidity::Invalid(PlacementFailure::OutsideMap)
        } else if hydrated_previews.get(preview_entity).is_err() {
            PlacementValidity::Pending
        } else {
            let hire_cost = Money(i64::from(staff_object.price_cents));
            if unlimited_zoo_cash.is_none() && zoo_cash.0 .0 < hire_cost.0 {
                PlacementValidity::Invalid(PlacementFailure::Unaffordable)
            } else {
                PlacementValidity::Valid { cost: hire_cost }
            }
        };
        if preview.validity != next_validity {
            preview.validity = next_validity;
        }
    }
}

pub(in crate::plugins::staff) fn begin_staff_placement_from_staff_purchase_choice(
    mut commands: Commands,
    mut purchase_choices: MessageReader<PurchaseChoice>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    construction_cursor: Query<Entity, With<ConstructionCursor>>,
    mut selected_construction_tools: MessageWriter<SelectConstructionTool>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Ok(construction_cursor) = construction_cursor.single() else {
        return;
    };
    for purchase_choice in purchase_choices.read() {
        let Some(staff_role) = definitions.find_staff(purchase_choice.definition) else {
            continue;
        };
        commands.entity(construction_cursor).insert((
            PendingStaffPlacement {
                staff_role: purchase_choice.definition,
            },
            ObjectPlacementPreviewRequest(openzt2_game_data::AssetId(staff_role.object.0)),
        ));
        selected_construction_tools.write(SelectConstructionTool(ConstructionTool::Placement));
    }
}

pub(in crate::plugins::staff) fn request_staff_hiring_from_confirmed_placement(
    mut commands: Commands,
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    ui_pointer_capture: Res<UiPointerCapture>,
    construction_tool: Res<ConstructionTool>,
    construction_cursor: Query<
        (
            Entity,
            &PendingStaffPlacement,
            &ObjectPlacementPreviewRequest,
        ),
        With<ConstructionCursor>,
    >,
    previews: Query<&ConstructionPreview>,
    mut staff_hiring_requests: MessageWriter<HireStaffRequest>,
    mut selected_construction_tools: MessageWriter<SelectConstructionTool>,
) {
    if *construction_tool != ConstructionTool::Placement
        || !primary_pointer.just_pressed
        || ui_pointer_capture.over_ui
    {
        return;
    }
    let Ok((cursor_entity, pending_placement, requested_prefab)) = construction_cursor.single()
    else {
        return;
    };
    let Some(valid_preview) = previews.iter().find(|preview| {
        preview.definition == requested_prefab.0
            && matches!(preview.validity, PlacementValidity::Valid { .. })
    }) else {
        return;
    };
    staff_hiring_requests.write(HireStaffRequest {
        role: pending_placement.staff_role,
        position: valid_preview.transform.translation,
    });
    commands
        .entity(cursor_entity)
        .remove::<(PendingStaffPlacement, ObjectPlacementPreviewRequest)>();
    selected_construction_tools.write(SelectConstructionTool(ConstructionTool::Inspect));
}

pub(in crate::plugins::staff) fn clear_staff_placement_after_leaving_placement_tool(
    mut commands: Commands,
    construction_tool: Res<ConstructionTool>,
    construction_cursors: Query<Entity, With<PendingStaffPlacement>>,
) {
    if !construction_tool.is_changed() || *construction_tool == ConstructionTool::Placement {
        return;
    }
    for construction_cursor in &construction_cursors {
        commands
            .entity(construction_cursor)
            .remove::<(PendingStaffPlacement, ObjectPlacementPreviewRequest)>();
    }
}
