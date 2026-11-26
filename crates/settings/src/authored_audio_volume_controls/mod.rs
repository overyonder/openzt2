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
    controls: Query<(&UiAudioSettingActions, &UiDocumentOwner, Option<&UiValue>)>,
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
        let volume = value.map(|value| value.0.clamp(0, 100) as u8);
        for record in actions.authored_action_records(document) {
            if record.trigger != activation.trigger {
                continue;
            }
            match record.action {
                UiAudioSettingAction::SetEightBitSound(enabled) => {
                    settings
                        .reborrow()
                        .map_unchanged(|settings| &mut settings.eight_bit_sound)
                        .set_if_neq(enabled);
                }
                channel => {
                    if let Some(volume) = volume {
                        settings
                            .reborrow()
                            .map_unchanged(|settings| audio_volume_mut(settings, channel))
                            .set_if_neq(volume);
                    }
                }
            }
        }
    }
}

pub(super) fn project_accepted_audio_volume_into_controls(
    settings: Res<AudioSettings>,
    mut controls: Query<(Ref<UiAudioVolumeControl>, &mut UiValue)>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut toggles: Query<
        (
            Ref<UiAudioSettingActions>,
            &UiDocumentOwner,
            &mut openzt2_interface_state::ui::authored_ui_selection_state::UiSelected,
        ),
        Without<UiAudioVolumeControl>,
    >,
) {
    for (control, mut value) in &mut controls {
        if settings.is_changed() || control.is_added() || value.is_added() {
            value.set_if_neq(UiValue(i64::from(*audio_volume(&settings, control.0))));
        }
    }
    for (actions, owner, mut selected) in &mut toggles {
        if !settings.is_changed()
            && !actions.is_changed()
            && !selected.is_added()
            && !documents.is_changed()
        {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if let Some(enabled) = actions
            .authored_action_records(document)
            .find_map(|record| match record.action {
                UiAudioSettingAction::SetEightBitSound(enabled) => Some(enabled),
                _ => None,
            })
        {
            selected.set_if_neq(
                openzt2_interface_state::ui::authored_ui_selection_state::UiSelected(
                    settings.eight_bit_sound == enabled,
                ),
            );
        }
    }
}

fn audio_volume(settings: &AudioSettings, channel: UiAudioSettingAction) -> &u8 {
    match channel {
        UiAudioSettingAction::SetEightBitSound(_) => {
            unreachable!("sound quality is not a volume channel")
        }
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
        UiAudioSettingAction::SetEightBitSound(_) => {
            unreachable!("sound quality is not a volume channel")
        }
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
