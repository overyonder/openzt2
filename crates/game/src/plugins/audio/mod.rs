//! ECS audio playback driven by per-document audio assets.

mod ambient_audio_emitter_playback;
mod audio_cue_playback;
mod audio_cue_resolution;
pub(crate) mod audio_environment_types;
pub(crate) mod audio_playback_message_types;
mod audio_player_spawning;
mod audio_settings_projection;
mod audio_soundscape_playback;
mod audio_stage_playback_projection;
mod audio_stop_request_execution;
mod deferred_audio_playback_start;
mod deterministic_audio_sampling;
mod direct_audio_clip_playback;
mod main_menu_music_playback;
mod water_audio_stage_projection;

use bevy::prelude::*;

use ambient_audio_emitter_playback::play_changed_allowed_ambient_audio_emitters;
use audio_cue_playback::execute_audio_cue_playback_requests;
use audio_playback_message_types::{PlayAudioClip, PlayAudioCue, StopAudioCue};
use audio_settings_projection::project_changed_audio_settings_into_live_audio_sinks;
use audio_soundscape_playback::{
    advance_audio_soundscape_cue_schedules,
    project_changed_audio_soundscapes_into_playback_and_schedules,
};
use audio_stage_playback_projection::project_changed_audio_stages_into_playback_requests;
use audio_stop_request_execution::execute_audio_stop_requests;
use deferred_audio_playback_start::start_deferred_audio_playback_after_delays_finish;
use direct_audio_clip_playback::execute_direct_audio_clip_playback_requests;
use water_audio_stage_projection::project_changed_water_regions_and_tanks_into_audio_stages;

pub(crate) struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayAudioCue>()
            .add_message::<PlayAudioClip>()
            .add_message::<StopAudioCue>()
            .add_systems(
                Update,
                (
                    project_changed_water_regions_and_tanks_into_audio_stages,
                    project_changed_audio_stages_into_playback_requests,
                    project_changed_audio_soundscapes_into_playback_and_schedules,
                    advance_audio_soundscape_cue_schedules,
                    play_changed_allowed_ambient_audio_emitters,
                    main_menu_music_playback::play_main_menu_music_when_audio_is_ready,
                    execute_audio_stop_requests,
                    execute_audio_cue_playback_requests,
                    execute_direct_audio_clip_playback_requests,
                    start_deferred_audio_playback_after_delays_finish,
                    project_changed_audio_settings_into_live_audio_sinks,
                )
                    .chain(),
            );
    }
}
