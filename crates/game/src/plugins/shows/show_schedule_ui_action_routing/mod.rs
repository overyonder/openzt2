use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::information::entity_selection_types::SelectedEntity;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::world_spawn::persistent_id_assignment::AssignPersistentId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    show_editor_interaction_types::{AddShowConfirmationPending, DeleteShowConfirmationPending},
    show_schedule_types::{
        ScheduledShowBreakRow, ScheduledShowRow, SelectedShowScheduleRow, ShowScheduleRowOrder,
    },
    show_schedule_ui_mutation_operations::{
        create_scheduled_show_row_after_authored_confirmation,
        create_show_schedule_break_row_for_selected_stage,
        delete_confirmed_show_schedule_row_and_close_editor_selection,
        move_selected_show_schedule_row_by_one_position,
        request_authored_confirmation_for_selected_show_schedule_row_deletion,
        show_selected_scheduled_show_in_read_only_editor,
    },
    show_stage_types::ShowStage,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShowActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct AuthoredShowScheduleActionRoutingInputs<'w, 's> {
    action_nodes: Query<'w, 's, (&'static UiShowActions, &'static UiDocumentOwner)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, Option<&'static ChildOf>)>,
    stages: Query<'w, 's, &'static ShowStage>,
    scheduler_selection: Query<'w, 's, &'static SelectedShowScheduleRow>,
    add_confirmations: Query<'w, 's, &'static AddShowConfirmationPending>,
    delete_confirmations: Query<'w, 's, &'static DeleteShowConfirmationPending>,
    scheduled: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static ScheduledShowRow>,
            Option<&'static ScheduledShowBreakRow>,
            Option<&'static ShowScheduleRowOrder>,
        ),
        Or<(With<ScheduledShowRow>, With<ScheduledShowBreakRow>)>,
    >,
    stage_members: Query<'w, 's, &'static WorldMember, With<ShowStage>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn route_authored_show_schedule_actions_into_rows_and_editor_selection(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected: Res<SelectedEntity>,
    inputs: AuthoredShowScheduleActionRoutingInputs,
    mut persistent_ids: MessageWriter<AssignPersistentId>,
) {
    let AuthoredShowScheduleActionRoutingInputs {
        action_nodes,
        roots,
        stages,
        scheduler_selection,
        add_confirmations,
        delete_confirmations,
        scheduled,
        stage_members,
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
        let selected_stage = selected.0.filter(|entity| stages.contains(*entity));
        let selected_schedule = selected.0.filter(|entity| scheduled.contains(*entity));
        if let Some(entry) = selected_schedule {
            commands
                .entity(controller)
                .insert(SelectedShowScheduleRow(entry));
        }
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
                            &scheduled,
                            &stage_members,
                            &mut commands,
                            &mut persistent_ids,
                        );
                    }
                }
                UiShowAction::RequestDeleteSelected => {
                    request_authored_confirmation_for_selected_show_schedule_row_deletion(
                        controller,
                        selected_schedule,
                        &scheduler_selection,
                        &mut commands,
                    );
                }
                UiShowAction::DeleteSelected => {
                    delete_confirmed_show_schedule_row_and_close_editor_selection(
                        controller,
                        &delete_confirmations,
                        &scheduled,
                        &mut commands,
                    );
                }
                UiShowAction::ViewSelected => {
                    show_selected_scheduled_show_in_read_only_editor(
                        controller,
                        selected_schedule,
                        &scheduler_selection,
                        &scheduled,
                        &mut commands,
                    );
                }
                UiShowAction::AddBreak => {
                    if let Some(stage) = selected_stage {
                        if let Some(world_definitions) =
                            active_world_definitions.get(&world_definition_assets)
                        {
                            create_show_schedule_break_row_for_selected_stage(
                                stage,
                                &scheduled,
                                world_definitions,
                                &stage_members,
                                &mut commands,
                                &mut persistent_ids,
                            );
                        }
                    }
                }
                UiShowAction::MoveSelectedUp => {
                    move_selected_show_schedule_row_by_one_position(
                        controller,
                        -1,
                        &scheduler_selection,
                        &scheduled,
                        &mut commands,
                    );
                }
                UiShowAction::MoveSelectedDown => {
                    move_selected_show_schedule_row_by_one_position(
                        controller,
                        1,
                        &scheduler_selection,
                        &scheduled,
                        &mut commands,
                    );
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
