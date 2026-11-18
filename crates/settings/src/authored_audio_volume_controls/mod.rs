use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::audio_settings::UiAudioSettingAction;
use openzt2_interface_state::{
    settings::audio_settings_types::AudioSettings,
    ui::{
        authored_ui_action_projection_components::{UiAudioSettingActions, UiAudioVolumeControl},
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiValue},
    },
};

pub(super) fn apply_authored_audio_volume_edits(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    controls: Query<(&UiAudioSettingActions, &UiDocumentOwner, &UiValue)>,
    roots: Query<&UiDocumentRoot>,
    mut settings: ResMut<AudioSettings>,
) {
    for activation in activations.read() {
        let Ok((actions, owner, value)) = controls.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let volume = value.0.clamp(0, 100) as u8;
        for record in actions.authored_action_records(document) {
            if record.trigger == activation.trigger
                && *audio_volume(&settings, record.action) != volume
            {
                *audio_volume_mut(&mut settings, record.action) = volume;
            }
        }
    }
}

pub(super) fn project_accepted_audio_volume_into_controls(
    settings: Res<AudioSettings>,
    mut controls: Query<(Ref<UiAudioVolumeControl>, &mut UiValue)>,
) {
    for (control, mut value) in &mut controls {
        if settings.is_changed() || control.is_added() || value.is_added() {
            value.set_if_neq(UiValue(i64::from(*audio_volume(&settings, control.0))));
        }
    }
}

fn audio_volume(settings: &AudioSettings, channel: UiAudioSettingAction) -> &u8 {
    match channel {
        UiAudioSettingAction::SetMasterVolume => &settings.master_volume_percent,
        UiAudioSettingAction::SetMusicVolume => &settings.music_volume_percent,
        UiAudioSettingAction::SetTwoDimensionalVolume => {
            &settings.two_dimensional_effect_volume_percent
        }
        UiAudioSettingAction::SetThreeDimensionalVolume => {
            &settings.three_dimensional_effect_volume_percent
        }
    }
}

fn audio_volume_mut(settings: &mut AudioSettings, channel: UiAudioSettingAction) -> &mut u8 {
    match channel {
        UiAudioSettingAction::SetMasterVolume => &mut settings.master_volume_percent,
        UiAudioSettingAction::SetMusicVolume => &mut settings.music_volume_percent,
        UiAudioSettingAction::SetTwoDimensionalVolume => {
            &mut settings.two_dimensional_effect_volume_percent
        }
        UiAudioSettingAction::SetThreeDimensionalVolume => {
            &mut settings.three_dimensional_effect_volume_percent
        }
    }
}
