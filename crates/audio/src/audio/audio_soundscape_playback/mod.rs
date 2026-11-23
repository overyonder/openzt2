use super::audio_request_lanes::{ProducedAudioRequest, SOUNDSCAPE_SETUP, SOUNDSCAPE_TIMER};
use super::world_audio_soundscape_membership::SoundscapeInputChanges;
use bevy::prelude::*;

use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset;

use super::{
    audio_environment_types::AudioSoundscape,
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
    deterministic_audio_sampling::sample_deterministic_f32_range,
};

#[derive(Component, Debug)]
pub(super) struct SoundscapeCueSchedule {
    delay: Timer,
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
    mut soundscapes: Query<(
        Entity,
        &AudioSoundscape,
        &WorldMember,
        Option<&mut SoundscapeCueSchedule>,
    )>,
    presets: Query<&WorldAudioPreset>,
    mut changes: SoundscapeInputChanges,
    mut requests: MessageWriter<ProducedAudioRequest<SOUNDSCAPE_SETUP>>,
) {
    let audio_assets_changed = audio_asset_index.is_changed();
    let audio_asset_view = audio_asset_index.get(&audio_assets);
    let delay_range = audio_asset_view
        .and_then(|view| view.ambient())
        .map_or([4.0, 6.0], |policy| policy.delay_seconds);
    let mut candidates = changes.affected_soundscapes();
    if audio_assets_changed {
        candidates.extend(soundscapes.iter().map(|(entity, ..)| entity));
    }
    let mut pending = soundscapes.iter_many_mut(candidates).matched();
    while let Some((soundscape_owner, selected_soundscape, member, schedule)) = pending.fetch_next()
    {
        let preset = presets.get(member.root).ok();
        requests.write(ProducedAudioRequest::Stop(StopAudioCue {
            emitter: Some(soundscape_owner),
            selection: selected_soundscape.selection,
        }));
        let soundscape = match preset {
            Some(preset) => audio_assets
                .get(&preset.0)
                .and_then(AudioAsset::soundscape_content),
            None => audio_asset_view.and_then(|view| view.soundscape(selected_soundscape.biome)),
        };
        let Some(soundscape) = soundscape else {
            if schedule.is_some() {
                commands
                    .entity(soundscape_owner)
                    .remove::<SoundscapeCueSchedule>();
            }
            continue;
        };
        if let Some(loop_cue) = soundscape.loop_cue {
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue: loop_cue,
                emitter: Some(soundscape_owner),
                selection: selected_soundscape.selection,
                priority: 48,
                force_looped: true,
            }));
        }
        if select_weighted_soundscape_cue(
            &soundscape.cues,
            selected_soundscape.selection,
            selected_soundscape.day_mask,
        )
        .is_some()
        {
            let next = SoundscapeCueSchedule {
                delay: Timer::new(
                    soundscape_delay(delay_range, selected_soundscape.selection),
                    TimerMode::Once,
                ),
                selection: selected_soundscape.selection,
            };
            if let Some(mut schedule) = schedule {
                *schedule = next;
            } else {
                commands.entity(soundscape_owner).insert(next);
            }
        } else {
            if schedule.is_some() {
                commands
                    .entity(soundscape_owner)
                    .remove::<SoundscapeCueSchedule>();
            }
        }
    }
}

pub(super) fn advance_audio_soundscape_cue_schedules(
    time: Res<Time>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    mut soundscapes: Query<(
        Entity,
        &AudioSoundscape,
        &WorldMember,
        &mut SoundscapeCueSchedule,
    )>,
    presets: Query<&WorldAudioPreset>,
    mut requests: MessageWriter<ProducedAudioRequest<SOUNDSCAPE_TIMER>>,
) {
    let audio_asset_view = audio_asset_index.get(&audio_assets);
    let ambient = audio_asset_view.and_then(|view| view.ambient());
    let delay_range = ambient.map_or([4.0, 6.0], |policy| policy.delay_seconds);
    let playback_probability = ambient
        .map_or(1.0, |policy| policy.probability)
        .clamp(0.0, 1.0);
    for (soundscape_owner, selected_soundscape, member, mut schedule) in &mut soundscapes {
        if !schedule.delay.tick(time.delta()).is_finished() {
            continue;
        }
        let soundscape = match presets.get(member.root) {
            Ok(preset) => audio_assets
                .get(&preset.0)
                .and_then(AudioAsset::soundscape_content),
            Err(_) => audio_asset_view.and_then(|view| view.soundscape(selected_soundscape.biome)),
        };
        let Some(soundscape) = soundscape else {
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
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue: cue.cue,
                emitter: Some(soundscape_owner),
                selection: schedule.selection,
                priority: 24,
                force_looped: false,
            }));
        }
        schedule.selection = schedule
            .selection
            .wrapping_add(0x9e37_79b9_7f4a_7c15)
            .rotate_left(27);
        let next_delay = soundscape_delay(delay_range, schedule.selection);
        schedule.delay.set_duration(next_delay);
        schedule.delay.reset();
    }
}

fn soundscape_delay(range: [f32; 2], selection: u64) -> std::time::Duration {
    let seconds = sample_deterministic_f32_range(range, selection);
    std::time::Duration::try_from_secs_f32(seconds).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::{
        audio::{AudioSoundscapeContent, AudioSoundscapeCue},
        AssetId as CueId,
    };

    #[test]
    fn saved_world_preset_drives_existing_loop_and_ambient_cue_playback() {
        let mut app = App::new();
        app.init_resource::<Assets<AudioAsset>>()
            .init_resource::<AudioAssets>()
            .init_resource::<Time>()
            .add_message::<AssetEvent<AudioAsset>>()
            .add_message::<ProducedAudioRequest<SOUNDSCAPE_SETUP>>()
            .add_message::<ProducedAudioRequest<SOUNDSCAPE_TIMER>>()
            .add_systems(
                Update,
                (
                    super::super::world_audio_soundscape_membership::bind_world_soundscape_membership,
                    project_changed_audio_soundscapes_into_playback_and_schedules,
                    advance_audio_soundscape_cue_schedules,
                )
                    .chain(),
            );
        let loop_cue = CueId::from_key("saved_loop");
        let cue = CueId::from_key("saved_bird");
        let preset = app.world_mut().resource_mut::<Assets<AudioAsset>>().add(
            AudioAsset::from_world_preset(
                AudioSoundscapeContent {
                    biome: CueId::from_key("saved"),
                    name: "saved".into(),
                    loop_cue: Some(loop_cue),
                    cues: vec![AudioSoundscapeCue {
                        cue,
                        weight: 1.0,
                        day_mask: 0b11,
                    }],
                },
                None,
            ),
        );
        let root = app.world_mut().spawn(WorldAudioPreset(preset)).id();
        let owner = app
            .world_mut()
            .spawn((
                AudioSoundscape {
                    biome: CueId::from_key("other"),
                    selection: 7,
                    day_mask: 0b11,
                },
                WorldMember { root },
            ))
            .id();
        app.update();
        let initial = app
            .world_mut()
            .resource_mut::<Messages<ProducedAudioRequest<SOUNDSCAPE_SETUP>>>()
            .drain()
            .filter_map(|request| match request {
                ProducedAudioRequest::Play(cue) => Some(cue),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].cue, loop_cue);
        assert!(initial[0].force_looped);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(10));
        app.update();
        let ambient = app
            .world_mut()
            .resource_mut::<Messages<ProducedAudioRequest<SOUNDSCAPE_TIMER>>>()
            .drain()
            .filter_map(|request| match request {
                ProducedAudioRequest::Play(cue) => Some(cue),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ambient.len(), 1);
        assert_eq!(ambient[0].cue, cue);
        assert!(!ambient[0].force_looped);
        app.world_mut()
            .entity_mut(root)
            .remove::<WorldAudioPreset>();
        app.update();
        assert!(app.world().get::<SoundscapeCueSchedule>(owner).is_none());
    }
}
