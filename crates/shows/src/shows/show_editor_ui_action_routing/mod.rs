use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::EnterImmersiveMode;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveMode;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
        ShowMixerDropdown,
    },
    show_schedule_types::{
        ScheduledShowPerformancePlan, ScheduledShowRow, ScheduledShowRowActivationState,
    },
    show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShowActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(SystemParam)]
pub(super) struct AuthoredShowEditorActionRoutingInputs<'w, 's> {
    action_nodes: Query<
        'w,
        's,
        (
            &'static UiShowActions,
            &'static UiDocumentOwner,
            Option<&'static Text>,
        ),
    >,
    document_roots: Query<'w, 's, (&'static UiDocumentRoot, Option<&'static ChildOf>)>,
    stages: Query<'w, 's, (), With<ShowStage>>,
    stage_open_states: Query<'w, 's, &'static mut ShowStageOpenState>,
    row_activation: Query<'w, 's, &'static mut ScheduledShowRowActivationState>,
    names: Query<'w, 's, &'static mut ShowName>,
    edited_shows: Query<'w, 's, &'static EditedShow>,
    editing: Query<'w, 's, Has<ShowEditing>>,
    schedule_rows: Query<'w, 's, &'static ScheduledShowRow, With<ScheduledShowPerformancePlan>>,
}

pub(super) fn route_authored_show_editor_actions_into_selection_and_immersive_mode_state(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    selected_entity: WorldUiSelection,
    inputs: AuthoredShowEditorActionRoutingInputs,
    mut enter_show_edit: MessageWriter<EnterImmersiveMode>,
) {
    let AuthoredShowEditorActionRoutingInputs {
        action_nodes,
        document_roots,
        stages,
        mut stage_open_states,
        mut row_activation,
        mut names,
        edited_shows,
        editing,
        schedule_rows,
    } = inputs;

    for activation in activations.read() {
        let Ok((authored_actions, document_owner, text)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok((document_root, parent)) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        let controller = parent.map_or(document_owner.0, ChildOf::parent);
        let selected_stage = selected_entity
            .subject()
            .filter(|entity| stages.contains(*entity));
        let selected_schedule_row = selected_entity
            .subject()
            .filter(|entity| schedule_rows.contains(*entity));

        for authored_action in authored_actions.authored_action_records(document) {
            if activation.trigger != authored_action.trigger {
                continue;
            }
            match &authored_action.action {
                UiShowAction::ToggleSelectedEnabled => {
                    if let Some(stage) = selected_stage {
                        if let Ok(mut state) = stage_open_states.get_mut(stage) {
                            *state = match *state {
                                ShowStageOpenState::Open => ShowStageOpenState::Closed,
                                ShowStageOpenState::Closed => ShowStageOpenState::Open,
                            };
                        } else {
                            commands.entity(stage).insert(ShowStageOpenState::Open);
                        }
                    }
                }
                UiShowAction::EditSelected => {
                    let stage = selected_schedule_row
                        .and_then(|row| schedule_rows.get(row).ok().map(|row| row.stage))
                        .or(selected_stage);
                    if let (Some(stage), Some(subject)) =
                        (stage, selected_schedule_row.or(selected_stage))
                    {
                        commands
                            .entity(controller)
                            .insert((EditedShow(subject), ShowEditing));
                        enter_show_edit.write(EnterImmersiveMode {
                            mode: ImmersiveMode::ShowEdit,
                            controller,
                            subject: Some(stage),
                        });
                    }
                }
                UiShowAction::CancelEdit => {
                    commands.entity(controller).remove::<(
                        ShowEditing,
                        ShowMixerDropdown,
                        AddShowConfirmationPending,
                        DeleteShowConfirmationPending,
                    )>();
                }
                UiShowAction::SetSelectedName => {
                    let edited_stage = edited_shows
                        .get(controller)
                        .ok()
                        .map(|edited_show| edited_show.0)
                        .or(selected_stage);
                    let submitted_name = text
                        .map(|text| text.0.trim())
                        .filter(|name| !name.is_empty());
                    if let (Some(stage), Some(name)) = (edited_stage, submitted_name) {
                        if let Ok(mut current) = names.get_mut(stage) {
                            if current.0 != name {
                                current.0.clear();
                                current.0.push_str(name);
                            }
                        } else {
                            commands.entity(stage).insert(ShowName(name.to_owned()));
                        }
                    }
                }
                UiShowAction::ShowMixerPanel => {
                    if let Some(stage) = selected_stage {
                        commands.entity(controller).insert(EditedShow(stage));
                    }
                }
                UiShowAction::ToggleEditing => {
                    if editing.get(controller).unwrap_or(false) {
                        commands.entity(controller).remove::<ShowEditing>();
                    } else {
                        commands.entity(controller).insert(ShowEditing);
                    }
                }
                UiShowAction::DismissMixerDropdown => {
                    commands.entity(controller).remove::<ShowMixerDropdown>();
                }
                UiShowAction::SetSelectedOpen { open } => {
                    if let Some(show) = selected_schedule_row.or(selected_stage) {
                        if stages.contains(show) {
                            let next = if *open {
                                ShowStageOpenState::Open
                            } else {
                                ShowStageOpenState::Closed
                            };
                            if let Ok(mut state) = stage_open_states.get_mut(show) {
                                state.set_if_neq(next);
                            } else {
                                commands.entity(show).insert(next);
                            }
                        } else if schedule_rows.contains(show) {
                            let next = if *open {
                                ScheduledShowRowActivationState::Enabled
                            } else {
                                ScheduledShowRowActivationState::Disabled
                            };
                            if let Ok(mut state) = row_activation.get_mut(show) {
                                state.set_if_neq(next);
                            } else {
                                commands.entity(show).insert(next);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
