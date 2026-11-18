use bevy::prelude::*;
use openzt2_game_assets::{
    graphics_settings_types::{GraphicsSettings, ReplaceGraphicsSettingsRequest},
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::{
    action::information::{InformationSettingAction, UiInformationAction},
    document::UiDocumentRole,
};
use openzt2_interface_state::{
    camera::camera_runtime_state_types::{CameraMouseLook, ZooCamera},
    settings::{
        display_settings_types::{DisplayMode, DisplaySettings, ReplaceDisplaySettingsRequest},
        online_message_policy_types::OnlineMessagePolicy,
        options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
    },
    ui::{
        authored_ui_action_projection_components::UiInformationActions,
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
        authored_ui_selection_state::UiSelected,
    },
};
use openzt2_ui::authored_ui_change_activation_dispatch::UiPreviousSelection;

pub(super) fn initialize_projected_options_settings_drafts(
    mut commands: Commands,
    roots: Query<(Entity, &UiDocumentRoot), Added<UiDocumentRoot>>,
    documents: Res<Assets<UiDocumentAsset>>,
    display: Res<DisplaySettings>,
    graphics: Res<GraphicsSettings>,
    online: Res<OnlineMessagePolicy>,
) {
    for (entity, root) in &roots {
        if documents.get(&root.document).is_some_and(|document| {
            matches!(
                document.canonical_ui_document().role,
                UiDocumentRole::Options | UiDocumentRole::InGameOptions
            )
        }) {
            commands
                .entity(entity)
                .insert(OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
                    display: *display,
                    graphics: *graphics,
                    message_of_the_day: online.enabled,
                    dirty_display: false,
                    dirty_graphics: false,
                });
        }
    }
}

pub(super) fn apply_authored_options_setting_edits(
    mut activations: MessageReader<UiNodeActivated>,
    nodes: Query<(&UiInformationActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut drafts: Query<&mut OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    accepted_display: Res<DisplaySettings>,
    accepted_graphics: Res<GraphicsSettings>,
    mut online: ResMut<OnlineMessagePolicy>,
    mut replace_display: MessageWriter<ReplaceDisplaySettingsRequest>,
    mut replace_graphics: MessageWriter<ReplaceGraphicsSettingsRequest>,
) {
    for activation in activations.read() {
        let Ok((actions, owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(mut draft) = drafts.get_mut(owner.0) else {
            continue;
        };
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        for record in actions
            .authored_action_records(document)
            .filter(|record| record.trigger == activation.trigger)
        {
            let UiInformationAction::ApplySetting { setting } = &record.action else {
                continue;
            };
            match setting {
                InformationSettingAction::Refresh | InformationSettingAction::Back => {
                    draft.set_if_neq(OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
                        display: *accepted_display,
                        graphics: *accepted_graphics,
                        message_of_the_day: online.enabled,
                        dirty_display: false,
                        dirty_graphics: false,
                    });
                }
                InformationSettingAction::HighestDetail => {
                    draft.graphics = GraphicsSettings::default();
                    draft.dirty_graphics = true;
                }
                InformationSettingAction::HighDetail => {
                    draft.graphics = GraphicsSettings::from_source_high_detail_preset();
                    draft.dirty_graphics = true;
                }
                InformationSettingAction::MediumDetail => {
                    draft.graphics = GraphicsSettings::from_source_medium_detail_preset();
                    draft.dirty_graphics = true;
                }
                InformationSettingAction::Fullscreen => {
                    draft.display.mode = DisplayMode::BorderlessFullscreen;
                    draft.dirty_display = true;
                }
                InformationSettingAction::Windowed => {
                    draft.display.mode = DisplayMode::Windowed;
                    draft.dirty_display = true;
                }
                InformationSettingAction::MessageOfTheDay(enabled) => {
                    draft.message_of_the_day = *enabled;
                }
                InformationSettingAction::Accept => {
                    if draft.dirty_display {
                        replace_display.write(ReplaceDisplaySettingsRequest(draft.display));
                        draft.dirty_display = false;
                    }
                    if draft.dirty_graphics {
                        replace_graphics.write(ReplaceGraphicsSettingsRequest(draft.graphics));
                        draft.dirty_graphics = false;
                    }
                    if online.enabled != draft.message_of_the_day {
                        online.enabled = draft.message_of_the_day;
                    }
                }
                InformationSettingAction::FreeMouseLook(_)
                | InformationSettingAction::CustomDetail => {}
            }
        }
    }
}

pub(super) fn apply_authored_mouse_look_settings(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    nodes: Query<(&UiInformationActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    cameras: Query<(Entity, Has<CameraMouseLook>), With<ZooCamera>>,
) {
    let mut next = None;
    for activation in activations.read() {
        let Ok((actions, owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        for record in actions
            .authored_action_records(document)
            .filter(|record| record.trigger == activation.trigger)
        {
            if let UiInformationAction::ApplySetting {
                setting: InformationSettingAction::FreeMouseLook(enabled),
            } = &record.action
            {
                next = Some(*enabled);
            }
        }
    }
    let Some(next) = next else { return };
    for (entity, current) in &cameras {
        match (current, next) {
            (false, true) => {
                commands.entity(entity).insert(CameraMouseLook);
            }
            (true, false) => {
                commands.entity(entity).remove::<CameraMouseLook>();
            }
            _ => {}
        }
    }
}

pub(super) fn project_options_draft_into_radio_buttons(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    drafts: Query<Ref<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>>,
    mut controls: Query<(
        Ref<UiInformationActions>,
        &UiDocumentOwner,
        &mut UiSelected,
        Option<&mut UiPreviousSelection>,
    )>,
) {
    for (actions, owner, mut selected, previous) in &mut controls {
        let Ok(draft) = drafts.get(owner.0) else {
            continue;
        };
        if !draft.is_changed() && !actions.is_added() {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        let Some(setting) = actions
            .authored_action_records(document)
            .find_map(|record| match &record.action {
                UiInformationAction::ApplySetting { setting } => Some(setting),
                _ => None,
            })
        else {
            continue;
        };
        if matches!(setting, InformationSettingAction::FreeMouseLook(_)) {
            continue;
        }
        let next = information_setting_matches_current_draft(setting, &draft);
        selected.set_if_neq(UiSelected(next));
        // The selected custom preset reveals its authored detail panels.
        if let Some(mut previous) =
            previous.filter(|_| !(next && *setting == InformationSettingAction::CustomDetail))
        {
            previous.synchronize_with_non_authored_selection_change(next);
        }
    }
}

pub(super) fn project_current_camera_mouse_look_into_controls(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    cameras: Query<Has<CameraMouseLook>, With<ZooCamera>>,
    changed_cameras: Query<
        (),
        (
            With<ZooCamera>,
            Or<(Added<ZooCamera>, Added<CameraMouseLook>)>,
        ),
    >,
    mut removed_mouse_look: RemovedComponents<CameraMouseLook>,
    mut removed_cameras: RemovedComponents<ZooCamera>,
    mut controls: Query<(
        Ref<UiInformationActions>,
        &UiDocumentOwner,
        &mut UiSelected,
        Option<&mut UiPreviousSelection>,
    )>,
) {
    let changed = !changed_cameras.is_empty()
        | (removed_mouse_look.read().count() > 0)
        | (removed_cameras.read().count() > 0);
    let enabled = cameras.iter().next().unwrap_or(true);
    for (actions, owner, mut selected, previous) in &mut controls {
        if !changed && !actions.is_added() {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        let Some(expected) = actions
            .authored_action_records(document)
            .find_map(|record| match &record.action {
                UiInformationAction::ApplySetting {
                    setting: InformationSettingAction::FreeMouseLook(enabled),
                } => Some(*enabled),
                _ => None,
            })
        else {
            continue;
        };
        let next = enabled == expected;
        selected.set_if_neq(UiSelected(next));
        if let Some(mut previous) = previous {
            previous.synchronize_with_non_authored_selection_change(next);
        }
    }
}

fn information_setting_matches_current_draft(
    authored_setting: &InformationSettingAction,
    draft_settings: &OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
) -> bool {
    match authored_setting {
        InformationSettingAction::HighestDetail => {
            draft_settings.graphics == GraphicsSettings::default()
        }
        InformationSettingAction::HighDetail => {
            draft_settings.graphics == GraphicsSettings::from_source_high_detail_preset()
        }
        InformationSettingAction::MediumDetail => {
            draft_settings.graphics == GraphicsSettings::from_source_medium_detail_preset()
        }
        InformationSettingAction::CustomDetail => {
            draft_settings.graphics != GraphicsSettings::default()
                && draft_settings.graphics != GraphicsSettings::from_source_high_detail_preset()
                && draft_settings.graphics != GraphicsSettings::from_source_medium_detail_preset()
        }
        InformationSettingAction::Fullscreen => {
            draft_settings.display.mode == DisplayMode::BorderlessFullscreen
        }
        InformationSettingAction::Windowed => draft_settings.display.mode == DisplayMode::Windowed,
        InformationSettingAction::MessageOfTheDay(enabled) => {
            draft_settings.message_of_the_day == *enabled
        }
        InformationSettingAction::FreeMouseLook(_)
        | InformationSettingAction::Refresh
        | InformationSettingAction::Accept
        | InformationSettingAction::Back => false,
    }
}
