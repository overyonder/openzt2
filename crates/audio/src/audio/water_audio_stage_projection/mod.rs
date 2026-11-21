use bevy::prelude::*;

use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::NaturalWaterRegion;

use super::audio_environment_types::AudioStage;

pub(super) fn project_changed_water_regions_and_tanks_into_audio_stages(
    mut commands: Commands,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    regions: Query<
        (Entity, Option<&NaturalWaterRegion>, Option<&TankGeometry>),
        Or<(With<NaturalWaterRegion>, With<TankGeometry>)>,
    >,
    changed: Query<Entity, Or<(Changed<NaturalWaterRegion>, Changed<TankGeometry>)>>,
    mut removed_water: RemovedComponents<NaturalWaterRegion>,
    mut removed_tanks: RemovedComponents<TankGeometry>,
    mut stages: Query<&mut AudioStage>,
) {
    let Some(view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let policy = view.water();
    let changed_catalogue = audio_asset_index.is_changed();
    let mut seen = bevy::ecs::entity::EntityHashSet::new();
    let candidates = changed_catalogue
        .then(|| regions.iter().map(|(entity, ..)| entity))
        .into_iter()
        .flatten()
        .chain(
            (!changed_catalogue)
                .then(|| changed.iter())
                .into_iter()
                .flatten(),
        )
        .chain(removed_water.read())
        .chain(removed_tanks.read())
        .filter(|&entity| seen.insert(entity));
    for entity in candidates {
        let next = policy
            .and_then(|policy| {
                let (_, region, tank) = regions.get(entity).ok()?;
                let stage_index = |area| {
                    usize::from(area >= policy.area_thresholds[0])
                        + usize::from(area >= policy.area_thresholds[1])
                };
                if let Some(tank) = tank {
                    policy.tank_stages[stage_index(tank.area)]
                } else {
                    let region = region?;
                    (region.area_m2 >= policy.lapping_minimum_area)
                        .then(|| policy.natural_stages[stage_index(region.area_m2)])
                        .flatten()
                }
            })
            .map(AudioStage);
        match (next, stages.get_mut(entity)) {
            (Some(next), Ok(mut stage)) => {
                stage.set_if_neq(next);
            }
            (Some(next), Err(_)) => {
                commands.entity(entity).insert(next);
            }
            (None, Ok(_)) => {
                commands.entity(entity).remove::<AudioStage>();
            }
            (None, Err(_)) => {}
        }
    }
}
