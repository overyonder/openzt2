use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_data::{audio::AudioCrowdPolicy, AssetId as CueId};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    audio::audio_environment_types::{AudioSoundscape, WorldAudioPreset},
    economy::monthly_finance_types::MonthlyFinanceHistory,
};

use super::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

#[derive(Component)]
pub(super) struct CrowdCuePlayback {
    band: Option<u8>,
    cue: Option<CueId>,
    selection: u64,
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
    finance: Option<Res<MonthlyFinanceHistory>>,
    presets: Query<&WorldAudioPreset>,
    soundscapes: Query<(
        Entity,
        &AudioSoundscape,
        &WorldMember,
        Option<&CrowdCuePlayback>,
    )>,
    mut play: MessageWriter<PlayAudioCue>,
    mut stop: MessageWriter<StopAudioCue>,
) {
    let Some(count) =
        finance.and_then(|history| history.current_month_finance().map(|row| row.active_users))
    else {
        return;
    };
    let Some(catalogue) = catalogue.get(&assets) else {
        return;
    };
    for (entity, soundscape, member, playback) in &soundscapes {
        let policy = match presets.get(member.root) {
            Ok(preset) => {
                let Some(preset) = assets.get(&preset.0) else {
                    continue;
                };
                preset.crowd_policy().or_else(|| catalogue.crowd())
            }
            Err(_) => catalogue.crowd(),
        };
        let band = policy.and_then(|policy| {
            select_crowd_band(count, playback.and_then(|playback| playback.band), policy)
        });
        let cue = band.and_then(|band| policy.and_then(|policy| policy.cues[usize::from(band)]));
        let selection = soundscape.selection ^ 0xc7d2_63a5_80e1_b49f;
        if playback.is_some_and(|playback| {
            playback.band == band && playback.cue == cue && playback.selection == selection
        }) {
            continue;
        }
        if let Some(playback) = playback {
            stop.write(StopAudioCue {
                emitter: Some(entity),
                selection: playback.selection,
            });
        }
        if let Some(cue) = cue {
            play.write(PlayAudioCue {
                cue,
                emitter: Some(entity),
                selection,
                priority: 48,
                force_looped: true,
            });
        }
        commands.entity(entity).insert(CrowdCuePlayback {
            band,
            cue,
            selection,
        });
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
            .insert_resource(history)
            .add_message::<PlayAudioCue>()
            .add_message::<StopAudioCue>()
            .add_systems(Update, project_current_crowd_into_audio_playback);
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
        let root = app.world_mut().spawn(WorldAudioPreset(preset)).id();
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
                .resource_mut::<Messages<PlayAudioCue>>()
                .drain()
                .map(|request| request.cue)
                .collect::<Vec<_>>()
        };
        assert_eq!(drain(&mut app), vec![cues[2]]);
        app.world_mut()
            .resource_mut::<MonthlyFinanceHistory>()
            .record_guest_arrival();
        app.update();
        assert_eq!(drain(&mut app), vec![cues[3]]);
        app.world_mut()
            .resource_mut::<MonthlyFinanceHistory>()
            .record_guest_departure();
        app.update();
        assert!(drain(&mut app).is_empty());
        for _ in 0..11 {
            app.world_mut()
                .resource_mut::<MonthlyFinanceHistory>()
                .record_guest_departure();
        }
        app.update();
        assert_eq!(drain(&mut app), vec![cues[2]]);
    }
}
