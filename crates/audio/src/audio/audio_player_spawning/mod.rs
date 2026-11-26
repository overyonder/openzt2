use super::spatial_cue_distance::SpatialCueDistance;
use super::stage_voice_acoustics::{PendingStageVoice, StageVoiceAcoustics};
use bevy::{
    audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume},
    prelude::*,
};
use openzt2_animals::animal_health::RampageAudioMix;
use openzt2_game_data::audio::AudioUsage;

use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AudioCueSelection {
    pub(super) emitter: Option<Entity>,
    pub(super) selection: u64,
    pub(super) usage: AudioUsage,
    pub(super) priority: u8,
}

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct AudioSourceGain(pub(super) f32);

#[derive(Component, Clone, Copy)]
pub(super) enum CueVoicePool {
    Ambient,
    Spatial,
    Nonspatial,
}

impl CueVoicePool {
    pub(super) fn for_cue(usage: AudioUsage, spatial: bool) -> Self {
        match usage {
            AudioUsage::Ambient => Self::Ambient,
            _ if spatial => Self::Spatial,
            _ => Self::Nonspatial,
        }
    }

    pub(super) const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Component)]
#[relationship(relationship_target = CueVoices)]
pub(super) struct CueVoiceOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = CueVoiceOf)]
pub(super) struct CueVoices(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = NonspatialCueVoices)]
pub(super) struct NonspatialCueVoiceOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = NonspatialCueVoiceOf)]
pub(super) struct NonspatialCueVoices(Vec<Entity>);

#[derive(Component)]
pub(super) struct NonspatialCueOwner;

#[derive(Component)]
#[relationship(relationship_target = WorldAudioVoices)]
pub(super) struct WorldAudioVoiceOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = WorldAudioVoiceOf, linked_spawn)]
pub(super) struct WorldAudioVoices(Vec<Entity>);

pub(super) fn rampage_voice_gain(mix: Option<&RampageAudioMix>, selection: u64) -> f32 {
    mix.map_or(1.0, |mix| {
        if selection == mix.loop_selection {
            mix.loop_gain
        } else {
            mix.other_sound_gain
        }
    })
}

#[derive(Default)]
pub(super) struct AudioVoiceAdmission {
    pub(super) pool: Option<Entity>,
    pub(super) nonspatial_owner: Option<Entity>,
    pub(super) lifetime_owner: Option<Entity>,
    pub(super) distance: Option<SpatialCueDistance>,
    pub(super) distance_gain: Option<f32>,
    pub(super) environment_gain: Option<f32>,
    pub(super) world: Option<Entity>,
    pub(super) rampage_gain: Option<f32>,
    pub(super) stage_acoustics: Option<StageVoiceAcoustics>,
}

#[derive(Component)]
pub(super) struct DeferredAudioPlayback {
    pub(super) timer: Timer,
    pub(super) clip: Handle<AudioSource>,
    pub(super) settings: PlaybackSettings,
}

pub(super) fn calculate_mixed_audio_gain(
    settings: &AudioSettings,
    usage: AudioUsage,
    spatial: bool,
    source_gain: f32,
) -> f32 {
    if settings.muted {
        return 0.0;
    }
    let category_volume_percent = if usage == AudioUsage::Music {
        settings.music_volume_percent
    } else if spatial {
        settings.three_dimensional_effect_volume_percent
    } else {
        settings.two_dimensional_effect_volume_percent
    };
    let gain = source_gain
        * f32::from(settings.master_volume_percent)
        * f32::from(category_volume_percent)
        / 10_000.0;
    if spatial {
        gain.max(0.0).powf(5.0 / 3.0)
    } else {
        gain
    }
}

pub(super) fn spawn_audio_player_or_deferred_playback(
    commands: &mut Commands,
    clip: Handle<AudioSource>,
    request: AudioCueSelection,
    playback_settings: PlaybackSettings,
    audio_settings: &AudioSettings,
    delay_seconds: f32,
    admission: AudioVoiceAdmission,
) -> Entity {
    let source_gain = playback_settings.volume.to_linear();
    let playback_settings = PlaybackSettings {
        volume: Volume::Linear(
            calculate_mixed_audio_gain(
                audio_settings,
                request.usage,
                request.emitter.is_some(),
                source_gain,
            ) * admission.distance_gain.unwrap_or(1.0)
                * admission.environment_gain.unwrap_or(1.0)
                * admission.rampage_gain.unwrap_or(1.0),
        ),
        ..playback_settings
    };
    let mut playback_entity = commands.spawn((request, AudioSourceGain(source_gain)));
    if let Some(distance) = admission.distance {
        playback_entity.insert(distance);
    }
    if let Some(pool) = admission.pool {
        playback_entity.insert(CueVoiceOf(pool));
    }
    if request.emitter.is_none() {
        if let Some(owner) = admission.nonspatial_owner {
            playback_entity.insert(NonspatialCueVoiceOf(owner));
        }
    }
    if let Some(root) = admission.world {
        playback_entity.insert(WorldAudioVoiceOf(root));
    }
    let uses_stage_acoustics = admission.stage_acoustics.is_some();
    if let Some(acoustics) = admission.stage_acoustics {
        playback_entity.insert(acoustics);
    }
    if delay_seconds > 0.0 {
        playback_entity.insert(DeferredAudioPlayback {
            timer: Timer::from_seconds(delay_seconds, TimerMode::Once),
            clip,
            settings: playback_settings,
        });
    } else if uses_stage_acoustics {
        playback_entity.insert((PendingStageVoice(clip), playback_settings));
    } else {
        playback_entity.insert((AudioPlayer(clip), playback_settings));
    }
    if request.emitter.is_some() {
        playback_entity.insert(Transform::IDENTITY);
    }
    if let Some(owner) = request
        .emitter
        .or(admission.lifetime_owner)
        .or(admission.nonspatial_owner)
    {
        playback_entity.insert(ChildOf(owner));
    }
    playback_entity.id()
}
