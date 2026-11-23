use super::audio_request_lanes::{ProducedAudioRequest, CROWD};
use super::world_audio_soundscape_membership::{SoundscapeInputChanges, WorldAudioSoundscapes};
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_data::{audio::AudioCrowdPolicy, AssetId as CueId};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    audio::audio_environment_types::{AudioSoundscape, WorldAudioPreset},
    economy::monthly_finance_types::MonthlyFinanceHistory,
};

use super::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

#[derive(Component, PartialEq, Eq)]
pub(super) struct CrowdCuePlayback {
    band: Option<u8>,
    cue: Option<CueId>,
    selection: u64,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct ActiveAudioGuestCount(i32);

#[derive(Component)]
pub(super) struct CrowdBandTransitionPending;

pub(super) fn publish_world_guest_count_for_crowd_audio(
    mut commands: Commands,
    mut worlds: Query<
        (
            Entity,
            &MonthlyFinanceHistory,
            Option<&mut ActiveAudioGuestCount>,
        ),
        Changed<MonthlyFinanceHistory>,
    >,
) {
    for (root, history, count) in &mut worlds {
        let next = ActiveAudioGuestCount(
            history
                .current_month_finance()
                .map_or(0, |row| row.active_users),
        );
        if let Some(mut count) = count {
            count.set_if_neq(next);
        } else {
            commands.entity(root).insert(next);
        }
    }
}

fn select_crowd_band(count: i32, current: Option<u8>, policy: &AudioCrowdPolicy) -> Option<u8> {
    let [quiet_high, small_low, small_high, medium_low, medium_high, large_low, large_high, huge_low] =
        policy.thresholds;
    if count < quiet_high && current != Some(0) {
        Some(0)
    } else if count > huge_low && matches!(current, Some(3) | None) {
        Some(4)
    } else if count > large_low && matches!(current, Some(2) | None) {
        Some(3)
    } else if count > medium_low && matches!(current, Some(1) | None) {
        Some(2)
    } else if (count > small_low && matches!(current, Some(0) | None))
        || (count < small_high && matches!(current, Some(2) | None))
    {
        Some(1)
    } else if count < medium_high && matches!(current, Some(3) | None) {
        Some(2)
    } else if count < large_high && matches!(current, Some(4) | None) {
        Some(3)
    } else {
        current
    }
}

pub(super) fn project_current_crowd_into_audio_playback(
    mut commands: Commands,
    assets: Res<Assets<AudioAsset>>,
    catalogue: Res<AudioAssets>,
    counts: Query<&ActiveAudioGuestCount>,
    changed_counts: Query<Entity, Changed<ActiveAudioGuestCount>>,
    transitioning: Query<Entity, With<CrowdBandTransitionPending>>,
    mut inputs: SoundscapeInputChanges,
    presets: Query<&WorldAudioPreset>,
    mut soundscapes: Query<(
        Entity,
        &AudioSoundscape,
        &WorldMember,
        Option<&mut CrowdCuePlayback>,
        Has<CrowdBandTransitionPending>,
    )>,
    mut requests: MessageWriter<ProducedAudioRequest<CROWD>>,
) {
    let catalogue_policy = catalogue
        .get(&assets)
        .and_then(|catalogue| catalogue.crowd());
    let mut candidates = inputs.affected_soundscapes();
    if catalogue.is_changed() {
        candidates.extend(soundscapes.iter().map(|(entity, ..)| entity));
    }
    candidates.extend(transitioning.iter());
    candidates.extend(
        changed_counts
            .iter()
            .filter_map(|root| inputs.world_soundscapes.get(root).ok())
            .flat_map(WorldAudioSoundscapes::iter),
    );
    let mut soundscapes = soundscapes.iter_many_mut(candidates).matched();
    while let Some((entity, soundscape, member, playback, has_pending_transition)) =
        soundscapes.fetch_next()
    {
        let Ok(&ActiveAudioGuestCount(count)) = counts.get(member.root) else {
            continue;
        };
        let policy = match presets.get(member.root) {
            Ok(preset) => {
                let Some(preset) = assets.get(&preset.0) else {
                    continue;
                };
                preset.crowd_policy().or(catalogue_policy)
            }
            Err(_) => catalogue_policy,
        };
        let band = policy.and_then(|policy| {
            select_crowd_band(
                count,
                playback.as_deref().and_then(|playback| playback.band),
                policy,
            )
        });
        let pending_transition =
            policy.is_some_and(|policy| select_crowd_band(count, band, policy) != band);
        match (pending_transition, has_pending_transition) {
            (true, false) => {
                commands.entity(entity).insert(CrowdBandTransitionPending);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<CrowdBandTransitionPending>();
            }
            _ => {}
        }
        let cue = band.and_then(|band| policy.and_then(|policy| policy.cues[usize::from(band)]));
        let selection = soundscape.selection ^ 0xc7d2_63a5_80e1_b49f;
        let next = CrowdCuePlayback {
            band,
            cue,
            selection,
        };
        if playback.as_deref() == Some(&next) {
            continue;
        }
        if let Some(playback) = playback.as_deref() {
            requests.write(ProducedAudioRequest::Stop(StopAudioCue {
                emitter: Some(entity),
                selection: playback.selection,
            }));
        }
        if let Some(cue) = cue {
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue,
                emitter: Some(entity),
                selection,
                priority: 48,
                force_looped: true,
            }));
        }
        if let Some(mut playback) = playback {
            playback.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::audio::AudioSoundscapeContent;
    use openzt2_zoo_state::economy::{money_types::Money, monthly_finance_types::MonthlyFinance};

    #[test]
    fn restored_general_users_drive_saved_crowd_cues_with_strict_hysteresis() {
        let mut app = App::new();
        let mut history = MonthlyFinanceHistory::default();
        assert!(history.restore_saved_monthly_finance_history(
            &[MonthlyFinance {
                active_users: 110,
                total_users: 110,
                ..Default::default()
            }],
            0,
            Money::ZERO,
            Money::ZERO,
            Money::ZERO,
            110,
            true
        ));
        app.init_resource::<Assets<AudioAsset>>()
            .init_resource::<AudioAssets>()
            .add_message::<AssetEvent<AudioAsset>>()
            .add_message::<ProducedAudioRequest<CROWD>>()
            .add_systems(Update, (
                super::super::world_audio_soundscape_membership::bind_world_soundscape_membership,
                publish_world_guest_count_for_crowd_audio,
                project_current_crowd_into_audio_playback,
            ).chain());
        let cues = ["quiet", "small", "medium", "large", "huge"].map(CueId::from_key);
        let preset = app.world_mut().resource_mut::<Assets<AudioAsset>>().add(
            AudioAsset::from_world_preset(
                AudioSoundscapeContent {
                    biome: CueId::default(),
                    name: "saved".into(),
                    loop_cue: None,
                    cues: vec![],
                },
                Some(AudioCrowdPolicy {
                    thresholds: [10, 15, 50, 60, 100, 110, 160, 170],
                    cues: cues.map(Some),
                }),
            ),
        );
        let root = app
            .world_mut()
            .spawn((WorldAudioPreset(preset), history))
            .id();
        app.world_mut().spawn((
            AudioSoundscape {
                biome: CueId::default(),
                selection: 3,
                day_mask: 3,
            },
            WorldMember { root },
        ));
        app.update();
        let drain = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Messages<ProducedAudioRequest<CROWD>>>()
                .drain()
                .filter_map(|request| match request {
                    ProducedAudioRequest::Play(request) => Some(request.cue),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(drain(&mut app), vec![cues[2]]);
        app.world_mut()
            .get_mut::<MonthlyFinanceHistory>(root)
            .unwrap()
            .record_guest_arrival();
        app.update();
        assert_eq!(drain(&mut app), vec![cues[3]]);
        app.world_mut()
            .get_mut::<MonthlyFinanceHistory>(root)
            .unwrap()
            .record_guest_departure();
        app.update();
        assert!(drain(&mut app).is_empty());
        for _ in 0..11 {
            app.world_mut()
                .get_mut::<MonthlyFinanceHistory>(root)
                .unwrap()
                .record_guest_departure();
        }
        app.update();
        assert_eq!(drain(&mut app), vec![cues[2]]);
    }
}
