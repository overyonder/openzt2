use bevy::prelude::*;
use openzt2_game_data::ui_document::action::audio_settings::UiAudioSettingAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_node_projection_components::UiValue,
    },
};

use super::audio_settings_types::{AudioSettings, ReplaceAudioSettingsRequest};
use crate::plugins::ui::authored_ui_action_projection_components::UiAudioSettingActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies the four source-authored audio sliders to the one accepted settings
/// resource. The loaded action table identifies the consumer; runtime never
/// switches on source widget names or generic setting strings.
pub(super) fn route_authored_audio_volume_actions_to_replacement_requests(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiAudioSettingActions, &UiDocumentOwner, Option<&UiValue>)>,
    roots: Query<&UiDocumentRoot>,
    current: Res<AudioSettings>,
    mut requests: MessageWriter<ReplaceAudioSettingsRequest>,
) {
    for activation in activations.read() {
        let Ok((range, owner, value)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            let next = match &record.action {
                UiAudioSettingAction::SetMasterVolume => AudioSettings {
                    master_volume_percent: value.map_or(current.master_volume_percent, |value| {
                        value.0.clamp(0, 100) as u8
                    }),
                    ..*current
                },
                UiAudioSettingAction::SetMusicVolume => AudioSettings {
                    music_volume_percent: value.map_or(current.music_volume_percent, |value| {
                        value.0.clamp(0, 100) as u8
                    }),
                    ..*current
                },
                UiAudioSettingAction::SetTwoDimensionalVolume => AudioSettings {
                    two_dimensional_effect_volume_percent: value
                        .map_or(current.two_dimensional_effect_volume_percent, |value| {
                            value.0.clamp(0, 100) as u8
                        }),
                    ..*current
                },
                UiAudioSettingAction::SetThreeDimensionalVolume => AudioSettings {
                    three_dimensional_effect_volume_percent: value
                        .map_or(current.three_dimensional_effect_volume_percent, |value| {
                            value.0.clamp(0, 100) as u8
                        }),
                    ..*current
                },
            };
            requests.write(ReplaceAudioSettingsRequest(next));
        }
    }
}

/// Publishes the accepted values when an options document appears, matching
/// the source mode's cached-volume publication without retaining a widget map.
pub(super) fn project_accepted_audio_volume_settings_into_authored_controls(
    settings: Res<AudioSettings>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut nodes: Query<(
        &UiAudioSettingActions,
        &UiDocumentOwner,
        Option<&mut UiValue>,
    )>,
) {
    for (range, owner, value) in &mut nodes {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let Some(record) = range.authored_action_records(document).next() else {
            continue;
        };
        let percent = match &record.action {
            UiAudioSettingAction::SetMasterVolume => settings.master_volume_percent,
            UiAudioSettingAction::SetMusicVolume => settings.music_volume_percent,
            UiAudioSettingAction::SetTwoDimensionalVolume => {
                settings.two_dimensional_effect_volume_percent
            }
            UiAudioSettingAction::SetThreeDimensionalVolume => {
                settings.three_dimensional_effect_volume_percent
            }
        };
        if let Some(mut value) = value {
            if value.0 != i64::from(percent) {
                value.0 = i64::from(percent);
            }
        }
    }
}
