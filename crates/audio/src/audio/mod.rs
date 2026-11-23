//! ECS audio playback driven by per-document audio assets.
use openzt2_zoo_state::audio::audio_environment_types;
use openzt2_zoo_state::audio::audio_playback_message_types;

mod ambient_audio_emitter_playback;
mod audio_cue_playback;
mod audio_cue_resolution;
mod audio_player_spawning;
mod audio_request_lanes;
mod audio_settings_projection;
mod audio_soundscape_playback;
mod audio_stage_playback_projection;
mod audio_stop_request_execution;
mod crowd_audio_playback;
mod deferred_audio_playback_start;
mod deterministic_audio_sampling;
mod direct_audio_clip_playback;
mod main_menu_music_playback;
mod spatial_cue_distance;
mod water_audio_stage_projection;
mod world_audio_soundscape_membership;

use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_world_state::application_schedule::WaterPresentationSet;

use audio_player_spawning::{CueVoicePool, NonspatialCueOwner};
use audio_request_lanes::{
    merge_produced_audio_requests, AdmitAudioCue, ProducedAudioRequest, AMBIENT_EMITTER, CROWD,
    MENU_MUSIC, SOUNDSCAPE_SETUP, SOUNDSCAPE_TIMER, WATER_STAGE,
};
use audio_stage_playback_projection::{
    advance_listener_stage_crossfades, ramp_listener_shore_lapping, select_listener_stage_ambience,
};
use main_menu_music_playback::{admit_pending_menu_music, begin_menu_music_visit, MenuMusicState};

use ambient_audio_emitter_playback::play_changed_allowed_ambient_audio_emitters;
use audio_cue_playback::execute_audio_cue_playback_requests;
use audio_playback_message_types::{
    PlayAudioClip, PlayAudioCue, PlayPositionedAudioCue, StopAudioCue,
};
use audio_settings_projection::project_changed_audio_settings_into_live_audio_sinks;
use audio_soundscape_playback::{
    advance_audio_soundscape_cue_schedules,
    project_changed_audio_soundscapes_into_playback_and_schedules,
};

use audio_stop_request_execution::execute_audio_stop_requests;
use deferred_audio_playback_start::start_deferred_audio_playback_after_delays_finish;
use direct_audio_clip_playback::execute_direct_audio_clip_playback_requests;
use water_audio_stage_projection::project_listener_audio_environment;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(bevy::audio::DefaultSpatialScale(
            bevy::audio::SpatialScale::new(1.0 / 30.0),
        ));
        app.world_mut().spawn(CueVoicePool::Ambient);
        app.world_mut().spawn(CueVoicePool::Spatial);
        app.world_mut()
            .spawn((CueVoicePool::Nonspatial, NonspatialCueOwner));
        app.add_computed_state::<MenuMusicState>()
            .add_message::<PlayAudioCue>()
            .add_message::<PlayPositionedAudioCue>()
            .add_message::<PlayAudioClip>()
            .add_message::<StopAudioCue>()
            .add_message::<ProducedAudioRequest<WATER_STAGE>>()
            .add_message::<ProducedAudioRequest<SOUNDSCAPE_SETUP>>()
            .add_message::<ProducedAudioRequest<SOUNDSCAPE_TIMER>>()
            .add_message::<ProducedAudioRequest<CROWD>>()
            .add_message::<ProducedAudioRequest<AMBIENT_EMITTER>>()
            .add_message::<ProducedAudioRequest<MENU_MUSIC>>()
            .add_message::<AdmitAudioCue>()
            .add_systems(OnEnter(MenuMusicState), begin_menu_music_visit)
            .add_systems(
                Update,
                (
                    project_listener_audio_environment.after(WaterPresentationSet::View),
                    select_listener_stage_ambience.after(project_listener_audio_environment),
                    advance_listener_stage_crossfades.after(select_listener_stage_ambience),
                    ramp_listener_shore_lapping.after(project_listener_audio_environment),
                )
                    .before(merge_produced_audio_requests),
            )
            .add_systems(
                Update,
                (
                    world_audio_soundscape_membership::bind_world_soundscape_membership
                        .before(project_changed_audio_soundscapes_into_playback_and_schedules)
                        .before(crowd_audio_playback::project_current_crowd_into_audio_playback)
                        .before(play_changed_allowed_ambient_audio_emitters),
                    crowd_audio_playback::publish_world_guest_count_for_crowd_audio
                        .before(crowd_audio_playback::project_current_crowd_into_audio_playback),
                ),
            )
            .add_systems(
                Update,
                (
                    (
                        project_changed_audio_soundscapes_into_playback_and_schedules,
                        advance_audio_soundscape_cue_schedules,
                    )
                        .chain(),
                    crowd_audio_playback::project_current_crowd_into_audio_playback,
                    play_changed_allowed_ambient_audio_emitters,
                    admit_pending_menu_music.run_if(
                        resource_changed::<AudioAssets>
                            .or_eager(on_message::<AssetEvent<AudioAsset>>),
                    ),
                )
                    .before_ignore_deferred(merge_produced_audio_requests),
            )
            .add_systems(
                Update,
                (
                    merge_produced_audio_requests,
                    execute_audio_stop_requests
                        .after(merge_produced_audio_requests)
                        .run_if(on_message::<StopAudioCue>),
                    audio_cue_playback::retire_empty_positioned_audio_emitters
                        .after(execute_audio_stop_requests),
                    audio_cue_playback::retire_empty_positioned_audio_groups
                        .after(audio_cue_playback::retire_empty_positioned_audio_emitters),
                    audio_cue_playback::forward_positioned_audio_cue_playback_requests
                        .after(WaterPresentationSet::View)
                        .after(audio_cue_playback::retire_empty_positioned_audio_groups)
                        .run_if(on_message::<PlayPositionedAudioCue>),
                    execute_audio_cue_playback_requests
                        .after(audio_cue_playback::forward_positioned_audio_cue_playback_requests)
                        .run_if(on_message::<AdmitAudioCue>),
                    execute_direct_audio_clip_playback_requests
                        .after(execute_audio_stop_requests)
                        .run_if(on_message::<PlayAudioClip>),
                    start_deferred_audio_playback_after_delays_finish
                        .after(execute_audio_cue_playback_requests),
                ),
            )
            .add_systems(Last, project_changed_audio_settings_into_live_audio_sinks);
    }
}
