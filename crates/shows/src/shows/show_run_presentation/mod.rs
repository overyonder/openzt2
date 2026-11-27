use std::sync::Arc;

use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{behavior::action::feedback::BehaviorFeedbackEntry, AssetId};
use openzt2_zoo_state::{
    audio::audio_playback_message_types::{PlayAudioCue, StopAudioCue},
    shows::running_show_types::ShowPhase,
};

use super::running_show_schedule::{ShowPhaseStarted, ShowRunCancelled};

#[derive(Message)]
pub struct ShowAudienceFeedback {
    pub guest: Entity,
    pub stage: Entity,
    pub entries: Arc<[BehaviorFeedbackEntry]>,
}

#[derive(Message)]
pub struct ShowMasterOfCeremoniesCue {
    pub run: Entity,
    pub stage: Entity,
    pub phase: ShowPhase,
    pub attachment: AssetId,
    pub performer_type: AssetId,
    pub token: AssetId,
    pub fade_ns: u64,
    pub replace_timeout_ns: u64,
}

pub(super) fn dispatch_run_presentation(
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut phases: MessageReader<ShowPhaseStarted>,
    mut cancelled: MessageReader<ShowRunCancelled>,
    mut play: Option<MessageWriter<PlayAudioCue>>,
    mut stop: Option<MessageWriter<StopAudioCue>>,
    mut masters: MessageWriter<ShowMasterOfCeremoniesCue>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        phases.clear();
        cancelled.clear();
        return;
    };
    let policy = definitions.show_presentation();
    let mut notify = |run, stage, phase| {
        if let Some(policy) = policy {
            let (index, default_token) = match phase {
                ShowPhase::Preshow => (0, "t_mcprepareshow"),
                ShowPhase::Conducting => (1, "t_mcexecuteshow"),
                ShowPhase::Postshow => (2, "t_mccleanupshow"),
                ShowPhase::BetweenShows => (3, "t_mcidle"),
            };
            let authored_or_default = |authored, fallback: &str| {
                if authored == AssetId::default() {
                    AssetId::from_key(fallback)
                } else {
                    authored
                }
            };
            let token =
                authored_or_default(policy.master_of_ceremonies_tokens[index], default_token);
            let performer_type = authored_or_default(policy.master_of_ceremonies_type, "mc");
            if performer_type == AssetId::from_key("") || token == AssetId::from_key("") {
                return;
            }
            masters.write(ShowMasterOfCeremoniesCue {
                run,
                stage,
                phase,
                attachment: authored_or_default(policy.master_of_ceremonies_spawn_node, "dock_mc"),
                performer_type,
                token,
                fade_ns: std::num::NonZeroU64::new(policy.master_of_ceremonies_fade_ns)
                    .map_or(15_000_000_000, std::num::NonZeroU64::get),
                replace_timeout_ns: std::num::NonZeroU64::new(
                    policy.master_of_ceremonies_replace_timeout_ns,
                )
                .map_or(30_000_000_000, std::num::NonZeroU64::get),
            });
        }
    };
    let mut presented = EntityHashMap::default();
    let mut songs_started = EntityHashSet::default();
    for phase in phases.read() {
        let (run, stage) = (phase.show, phase.stage);
        if presented.insert(run, phase.phase) != Some(phase.phase) {
            notify(run, stage, phase.phase);
        }
        if (phase.phase == ShowPhase::Preshow || phase.initial) && songs_started.insert(run) {
            let count = definitions.show_audio_cues().count();
            if let Some(cue) = definitions
                .show_audio_cues()
                .nth((run.to_bits() as usize).checked_rem(count).unwrap_or(0))
            {
                if let Some(play) = play.as_mut() {
                    play.write(PlayAudioCue {
                        cue: if cue.sound == AssetId::default() {
                            cue.id
                        } else {
                            cue.sound
                        },
                        emitter: Some(stage),
                        selection: run.to_bits(),
                        priority: 0,
                        force_looped: true,
                    });
                }
            }
        }
        if phase.phase == ShowPhase::BetweenShows {
            if let Some(stop) = stop.as_mut() {
                stop.write(StopAudioCue {
                    emitter: Some(stage),
                    selection: run.to_bits(),
                });
            }
        }
    }
    for exit in cancelled.read() {
        notify(exit.show, exit.stage, ShowPhase::Postshow);
        if let Some(stop) = stop.as_mut() {
            stop.write(StopAudioCue {
                emitter: Some(exit.stage),
                selection: exit.show.to_bits(),
            });
        }
    }
}
