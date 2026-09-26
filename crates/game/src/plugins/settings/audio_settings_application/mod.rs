use bevy::prelude::*;

use super::audio_settings_types::{AudioSettings, ReplaceAudioSettingsRequest};

pub(super) fn replace_accepted_audio_settings_from_requests(
    mut requests: MessageReader<ReplaceAudioSettingsRequest>,
    mut current: ResMut<AudioSettings>,
) {
    for request in requests.read() {
        *current = request.0;
    }
}
