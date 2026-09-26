use bevy::prelude::*;

use crate::assets::audio::audio_asset_types::{AudioAsset, AudioAssets};

use super::{
    audio_environment_types::AudioSoundscape,
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
    deterministic_audio_sampling::sample_deterministic_f32_range,
};

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct SoundscapeCueSchedule {
    remaining_seconds: f32,
    selection: u64,
}

fn select_weighted_soundscape_cue(
    cues: &[openzt2_game_data::audio::AudioSoundscapeCue],
    selection: u64,
    day_mask: u8,
) -> Option<&openzt2_game_data::audio::AudioSoundscapeCue> {
    let cue_is_eligible = |cue: &&openzt2_game_data::audio::AudioSoundscapeCue| {
        cue.day_mask == 0 || cue.day_mask & day_mask != 0
    };
    let total_weight = cues
        .iter()
        .filter(cue_is_eligible)
        .map(|cue| cue.weight.max(0.0))
        .sum::<f32>();
    if total_weight <= 0.0 {
        return None;
    }
    let mut selected_weight_offset = sample_deterministic_f32_range([0.0, total_weight], selection);
    cues.iter().filter(cue_is_eligible).find(|cue| {
        selected_weight_offset -= cue.weight.max(0.0);
        selected_weight_offset <= 0.0
    })
}

pub(super) fn project_changed_audio_soundscapes_into_playback_and_schedules(
    mut commands: Commands,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    soundscapes: Query<(Entity, Ref<AudioSoundscape>)>,
    mut play_requests: MessageWriter<PlayAudioCue>,
    mut stop_requests: MessageWriter<StopAudioCue>,
) {
    let audio_assets_changed = audio_asset_index.is_changed();
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let delay_range = audio_asset_view
        .ambient()
        .map_or([4.0, 6.0], |policy| policy.delay_seconds);
    for (soundscape_owner, selected_soundscape) in &soundscapes {
        if !audio_assets_changed
            && !selected_soundscape.is_added()
            && !selected_soundscape.is_changed()
        {
            continue;
        }
        stop_requests.write(StopAudioCue {
            emitter: Some(soundscape_owner),
            selection: selected_soundscape.selection,
        });
        let Some(soundscape) = audio_asset_view.soundscape(selected_soundscape.biome) else {
            commands
                .entity(soundscape_owner)
                .remove::<SoundscapeCueSchedule>();
            continue;
        };
        if let Some(loop_cue) = soundscape.loop_cue {
            play_requests.write(PlayAudioCue {
                cue: loop_cue,
                emitter: Some(soundscape_owner),
                selection: selected_soundscape.selection,
                priority: 48,
                force_looped: true,
            });
        }
        if select_weighted_soundscape_cue(
            &soundscape.cues,
            selected_soundscape.selection,
            selected_soundscape.day_mask,
        )
        .is_some()
        {
            commands
                .entity(soundscape_owner)
                .insert(SoundscapeCueSchedule {
                    remaining_seconds: sample_deterministic_f32_range(
                        delay_range,
                        selected_soundscape.selection,
                    ),
                    selection: selected_soundscape.selection,
                });
        } else {
            commands
                .entity(soundscape_owner)
                .remove::<SoundscapeCueSchedule>();
        }
    }
}

pub(super) fn advance_audio_soundscape_cue_schedules(
    time: Res<Time>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    mut soundscapes: Query<(Entity, &AudioSoundscape, &mut SoundscapeCueSchedule)>,
    mut play_requests: MessageWriter<PlayAudioCue>,
) {
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let delay_range = audio_asset_view
        .ambient()
        .map_or([4.0, 6.0], |policy| policy.delay_seconds);
    let playback_probability = audio_asset_view
        .ambient()
        .map_or(1.0, |policy| policy.probability)
        .clamp(0.0, 1.0);
    for (soundscape_owner, selected_soundscape, mut schedule) in &mut soundscapes {
        schedule.remaining_seconds -= time.delta_secs();
        if schedule.remaining_seconds > 0.0 {
            continue;
        }
        let Some(soundscape) = audio_asset_view.soundscape(selected_soundscape.biome) else {
            continue;
        };
        let Some(cue) = select_weighted_soundscape_cue(
            &soundscape.cues,
            schedule.selection,
            selected_soundscape.day_mask,
        ) else {
            continue;
        };
        if sample_deterministic_f32_range([0.0, 1.0], schedule.selection.rotate_left(41))
            <= playback_probability
        {
            play_requests.write(PlayAudioCue {
                cue: cue.cue,
                emitter: Some(soundscape_owner),
                selection: schedule.selection,
                priority: 24,
                force_looped: false,
            });
        }
        schedule.selection = schedule
            .selection
            .wrapping_add(0x9e37_79b9_7f4a_7c15)
            .rotate_left(27);
        schedule.remaining_seconds =
            sample_deterministic_f32_range(delay_range, schedule.selection);
    }
}
