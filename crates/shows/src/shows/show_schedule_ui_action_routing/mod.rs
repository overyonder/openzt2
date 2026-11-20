use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
    },
    show_schedule_types::{SelectedShowScheduleRow, ShowStageRows},
    show_schedule_ui_mutation_operations::{
        create_scheduled_show_row_after_authored_confirmation,
        create_show_schedule_break_row_for_selected_stage,
        delete_confirmed_show_schedule_row_and_close_editor_selection,
        move_show_schedule_row_by_one_position, AuthoredShowScheduleRowQuery,
    },
    show_stage_types::ShowStage,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShowActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct AuthoredShowScheduleActionRoutingInputs<'w, 's> {
    action_nodes: Query<'w, 's, (&'static UiShowActions, &'static UiDocumentOwner)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, Option<&'static ChildOf>)>,
    stages: Query<'w, 's, &'static ShowStage>,
    scheduler_selection: Query<'w, 's, &'static SelectedShowScheduleRow>,
    add_confirmations: Query<'w, 's, &'static AddShowConfirmationPending>,
    delete_confirmations: Query<'w, 's, &'static DeleteShowConfirmationPending>,
    scheduled: AuthoredShowScheduleRowQuery<'w, 's>,
    stage_rows: Query<'w, 's, &'static ShowStageRows>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn route_authored_show_schedule_actions_into_rows_and_editor_selection(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected: WorldUiSelection,
    inputs: AuthoredShowScheduleActionRoutingInputs,
) {
    let AuthoredShowScheduleActionRoutingInputs {
        action_nodes,
        roots,
        stages,
        scheduler_selection,
        add_confirmations,
        delete_confirmations,
        mut scheduled,
        stage_rows,
    } = inputs;
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let controller = parent.map_or(owner.0, ChildOf::parent);
        let selected_stage = selected.subject().filter(|entity| stages.contains(*entity));
        let selected_schedule = selected
            .subject()
            .filter(|entity| scheduled.contains(*entity));
        if let Some(entry) = selected_schedule {
            commands
                .entity(controller)
                .insert(SelectedShowScheduleRow(entry));
        }
        let schedule_selection = selected_schedule.or_else(|| {
            scheduler_selection
                .get(controller)
                .ok()
                .map(|selection| selection.0)
        });
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiShowAction::RequestAddShow => {
                    if let Some(stage) = selected_stage {
                        commands
                            .entity(controller)
                            .insert(AddShowConfirmationPending { stage });
                    }
                }
                UiShowAction::AddShow => {
                    if let Some(world_definitions) =
                        active_world_definitions.get(&world_definition_assets)
                    {
                        create_scheduled_show_row_after_authored_confirmation(
                            controller,
                            &add_confirmations,
                            &stages,
                            world_definitions,
                            &mut commands,
                        );
                    }
                }
                UiShowAction::RequestDeleteSelected => {
                    if let Some(entry) = schedule_selection {
                        commands
                            .entity(controller)
                            .insert(DeleteShowConfirmationPending { entry });
                    }
                }
                UiShowAction::DeleteSelected => {
                    delete_confirmed_show_schedule_row_and_close_editor_selection(
                        controller,
                        &delete_confirmations,
                        &mut scheduled,
                        &stage_rows,
                        &mut commands,
                    );
                }
                UiShowAction::ViewSelected => {
                    if let Some(row) = schedule_selection
                        .filter(|&row| scheduled.get(row).is_ok_and(|(_, _, is_show)| is_show))
                    {
                        commands
                            .entity(controller)
                            .insert(EditedShow(row))
                            .remove::<ShowEditing>();
                    }
                }
                UiShowAction::AddBreak => {
                    if let Some(stage) = selected_stage {
                        if let Some(world_definitions) =
                            active_world_definitions.get(&world_definition_assets)
                        {
                            create_show_schedule_break_row_for_selected_stage(
                                stage,
                                world_definitions,
                                &mut commands,
                            );
                        }
                    }
                }
                UiShowAction::MoveSelectedUp | UiShowAction::MoveSelectedDown => {
                    if let Some(row) = schedule_selection {
                        let direction = if matches!(record.action, UiShowAction::MoveSelectedUp) {
                            -1
                        } else {
                            1
                        };
                        move_show_schedule_row_by_one_position(
                            row,
                            direction,
                            &mut scheduled,
                            &stage_rows,
                        );
                    }
                }
                UiShowAction::ToggleSelectedEnabled
                | UiShowAction::EditSelected
                | UiShowAction::CancelEdit
                | UiShowAction::SetSelectedName
                | UiShowAction::ShowMixerPanel
                | UiShowAction::ToggleEditing
                | UiShowAction::DismissMixerDropdown
                | UiShowAction::SetSelectedOpen { .. }
                | UiShowAction::PurchaseSelectedPlatformUpgrade
                | UiShowAction::SelectPlatformUpgrade { .. }
                | UiShowAction::ResolvePlatformDeletion { .. } => {}
            }
        }
    }
}
