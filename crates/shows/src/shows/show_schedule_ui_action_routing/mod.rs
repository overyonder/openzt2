use bevy::{ecs::system::SystemParam, platform::collections::HashMap, prelude::*};
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShowActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
    },
    show_schedule_types::{
        ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRowActivationState,
        SelectedShowScheduleRow,
    },
    show_schedule_ui_mutation_operations::{ShowScheduleEdits, ShowScheduleRows},
    show_stage_types::ShowStage,
    show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks,
};

#[derive(SystemParam)]
pub(super) struct AuthoredShowScheduleActionRoutingInputs<'w, 's> {
    actions: Query<'w, 's, (&'static UiShowActions, &'static UiDocumentOwner)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, Option<&'static ChildOf>)>,
    stages: Query<'w, 's, (&'static ShowStage, Option<&'static WorldMember>)>,
    controller: Query<
        'w,
        's,
        (
            Option<&'static SelectedShowScheduleRow>,
            Option<&'static AddShowConfirmationPending>,
            Option<&'static DeleteShowConfirmationPending>,
        ),
    >,
    schedules: ShowScheduleRows<'w, 's>,
}

#[derive(Default)]
struct PendingScheduleControllerChanges {
    selected_row: Option<Entity>,
    add_stage: Option<Entity>,
    delete_row: Option<Entity>,
}

pub(super) fn route_authored_show_schedule_actions_into_rows_and_editor_selection(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected: WorldUiSelection,
    mut inputs: AuthoredShowScheduleActionRoutingInputs,
) {
    if activations.is_empty() {
        return;
    }
    let definitions = active_world_definitions.get(&world_definition_assets);
    let selected_stage = selected
        .subject()
        .filter(|&entity| inputs.stages.contains(entity));
    let mut edits = ShowScheduleEdits::default();
    let mut controllers = HashMap::<Entity, PendingScheduleControllerChanges>::new();
    for activation in activations.read() {
        let Ok((range, owner)) = inputs.actions.get(activation.node) else {
            continue;
        };
        let Ok((root, parent)) = inputs.roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let controller = parent.map_or(owner.0, ChildOf::parent);
        let current = controllers.entry(controller).or_insert_with(|| {
            let (selection, add, delete) = inputs.controller.get(controller).unwrap_or_default();
            let mut selected_row = selection.map(|selection| selection.0);
            if let Some(row) = selected
                .subject()
                .filter(|&row| edits.row_facts(row, &inputs.schedules).is_some())
            {
                if selected_row != Some(row) {
                    commands
                        .entity(controller)
                        .insert(SelectedShowScheduleRow(row));
                }
                selected_row = Some(row);
            }
            PendingScheduleControllerChanges {
                selected_row,
                add_stage: add.map(|add| add.stage),
                delete_row: delete.map(|delete| delete.entry),
            }
        });
        for record in range.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match record.action {
                UiShowAction::RequestAddShow => {
                    if let Some(stage) = selected_stage {
                        current.add_stage = Some(stage);
                        commands
                            .entity(controller)
                            .insert(AddShowConfirmationPending { stage });
                    }
                }
                UiShowAction::AddShow => {
                    let (Some(stage), Some(definitions)) = (current.add_stage, definitions) else {
                        continue;
                    };
                    let Ok((stage_facts, member)) = inputs.stages.get(stage) else {
                        continue;
                    };
                    let Some(definition) =
                        definitions.find_show_stage(stage_facts.show_stage_definition)
                    else {
                        continue;
                    };
                    current.add_stage = None;
                    let row = edits.append(
                        stage,
                        member,
                        true,
                        (
                            ScheduledShowPerformancePlan {
                                trick_performance_capacity: definition.schedule_slots,
                            },
                            ScheduledShowRowActivationState::Enabled,
                        ),
                        &inputs.schedules,
                        &mut commands,
                    );
                    current.selected_row = Some(row);
                    commands
                        .entity(controller)
                        .remove::<AddShowConfirmationPending>()
                        .insert((SelectedShowScheduleRow(row), EditedShow(row), ShowEditing));
                }
                UiShowAction::RequestDeleteSelected => {
                    if let Some(entry) = current
                        .selected_row
                        .filter(|&row| edits.row_facts(row, &inputs.schedules).is_some())
                    {
                        current.delete_row = Some(entry);
                        commands
                            .entity(controller)
                            .insert(DeleteShowConfirmationPending { entry });
                    }
                }
                UiShowAction::DeleteSelected => {
                    if let Some(row) = current.delete_row.take() {
                        edits.delete(row, &inputs.schedules, &mut commands);
                        current.selected_row = None;
                        commands.entity(controller).try_remove::<(
                            SelectedShowScheduleRow,
                            DeleteShowConfirmationPending,
                            EditedShow,
                            ShowEditing,
                        )>();
                    }
                }
                UiShowAction::ViewSelected => {
                    if let Some(row) = current.selected_row.filter(|&row| {
                        edits
                            .row_facts(row, &inputs.schedules)
                            .is_some_and(|(_, is_show)| is_show)
                    }) {
                        commands
                            .entity(controller)
                            .insert(EditedShow(row))
                            .remove::<ShowEditing>();
                    }
                }
                UiShowAction::AddBreak => {
                    let (Some(stage), Some(definitions)) = (selected_stage, definitions) else {
                        continue;
                    };
                    let Ok((_, member)) = inputs.stages.get(stage) else {
                        continue;
                    };
                    let duration_ticks = definitions.show_scheduling().map_or(1, |schedule| {
                        convert_authored_nanoseconds_to_fixed_simulation_ticks(
                            schedule.between_show_ns,
                            definitions,
                        )
                        .max(1)
                    });
                    edits.append(
                        stage,
                        member,
                        false,
                        ScheduledShowBreakRow { duration_ticks },
                        &inputs.schedules,
                        &mut commands,
                    );
                }
                UiShowAction::MoveSelectedUp | UiShowAction::MoveSelectedDown => {
                    if let Some(row) = current.selected_row {
                        let direction = if matches!(record.action, UiShowAction::MoveSelectedUp) {
                            -1
                        } else {
                            1
                        };
                        edits.move_adjacent(row, direction, &inputs.schedules);
                    }
                }
                _ => {}
            }
        }
    }
    edits.finish(&mut inputs.schedules, &mut commands);
}
