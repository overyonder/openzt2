use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::UiShowAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        immersive_modes::{
            immersive_mode_message_types::EnterImmersiveMode,
            immersive_mode_state_types::ImmersiveMode,
        },
        information::entity_selection_types::SelectedEntity,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

use super::{
    show_editor_interaction_types::{
        AddShowConfirmationPending, DeleteShowConfirmationPending, EditedShow, ShowEditing,
        ShowMixerDropdown,
    },
    show_schedule_types::{
        ScheduledShowBreakRow, ScheduledShowRow, ScheduledShowRowActivationState,
    },
    show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShowActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

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
    stage_open_states: Query<'w, 's, &'static ShowStageOpenState>,
    edited_shows: Query<'w, 's, &'static EditedShow>,
    editing: Query<'w, 's, Has<ShowEditing>>,
    schedule_rows: Query<'w, 's, (), Or<(With<ScheduledShowRow>, With<ScheduledShowBreakRow>)>>,
    scheduled_show_rows: Query<'w, 's, (), With<ScheduledShowRow>>,
}

pub(super) fn route_authored_show_editor_actions_into_selection_and_immersive_mode_state(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    selected_entity: Res<SelectedEntity>,
    inputs: AuthoredShowEditorActionRoutingInputs,
    mut enter_show_edit: MessageWriter<EnterImmersiveMode>,
) {
    let AuthoredShowEditorActionRoutingInputs {
        action_nodes,
        document_roots,
        stages,
        stage_open_states,
        edited_shows,
        editing,
        schedule_rows,
        scheduled_show_rows,
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
        let selected_stage = selected_entity.0.filter(|entity| stages.contains(*entity));
        let selected_schedule_row = selected_entity
            .0
            .filter(|entity| schedule_rows.contains(*entity));

        for authored_action in authored_actions.authored_action_records(document) {
            if activation.trigger != authored_action.trigger {
                continue;
            }
            match &authored_action.action {
                UiShowAction::ToggleSelectedEnabled => {
                    if let Some(stage) = selected_stage {
                        let next_open_state = stage_open_states.get(stage).map_or(
                            ShowStageOpenState::Open,
                            |open_state| match open_state {
                                ShowStageOpenState::Open => ShowStageOpenState::Closed,
                                ShowStageOpenState::Closed => ShowStageOpenState::Open,
                            },
                        );
                        commands.entity(stage).insert(next_open_state);
                    }
                }
                UiShowAction::EditSelected => {
                    if let Some(show) = selected_schedule_row.or(selected_stage) {
                        commands
                            .entity(controller)
                            .insert((EditedShow(show), ShowEditing));
                        enter_show_edit.write(EnterImmersiveMode {
                            mode: ImmersiveMode::ShowEdit,
                            controller,
                            subject: Some(show),
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
                        commands.entity(stage).insert(ShowName(name.to_owned()));
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
                            commands.entity(show).insert(if *open {
                                ShowStageOpenState::Open
                            } else {
                                ShowStageOpenState::Closed
                            });
                        } else if scheduled_show_rows.contains(show) {
                            commands.entity(show).insert(if *open {
                                ScheduledShowRowActivationState::Enabled
                            } else {
                                ScheduledShowRowActivationState::Disabled
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
