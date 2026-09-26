use bevy::prelude::*;

use crate::{
    assets::audio::audio_asset_types::{AudioAsset, AudioAssets},
    plugins::{
        aquatic::aquatic_simulation_types::TankGeometry,
        terrain::terrain_water_presentation_types::NaturalWaterRegion,
    },
};

use super::audio_environment_types::AudioStage;

pub(super) fn project_changed_water_regions_and_tanks_into_audio_stages(
    mut commands: Commands,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    natural_water_regions: Query<(Entity, Ref<NaturalWaterRegion>)>,
    tanks: Query<(Entity, Ref<TankGeometry>)>,
) {
    let audio_assets_changed = audio_asset_index.is_changed();
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let water_policy = audio_asset_view.water();
    for (water_region_entity, water_region) in &natural_water_regions {
        if !audio_assets_changed && !water_region.is_added() && !water_region.is_changed() {
            continue;
        }
        let Some(water_policy) = water_policy else {
            commands.entity(water_region_entity).remove::<AudioStage>();
            continue;
        };
        if water_region.area_m2 < water_policy.lapping_minimum_area {
            commands.entity(water_region_entity).remove::<AudioStage>();
            continue;
        }
        let stage_index = usize::from(water_region.area_m2 >= water_policy.area_thresholds[0])
            + usize::from(water_region.area_m2 >= water_policy.area_thresholds[1]);
        if let Some(stage) = water_policy.natural_stages[stage_index] {
            commands
                .entity(water_region_entity)
                .insert(AudioStage(stage));
        } else {
            commands.entity(water_region_entity).remove::<AudioStage>();
        }
    }
    for (tank_entity, tank_geometry) in &tanks {
        if !audio_assets_changed && !tank_geometry.is_added() && !tank_geometry.is_changed() {
            continue;
        }
        let Some(water_policy) = water_policy else {
            commands.entity(tank_entity).remove::<AudioStage>();
            continue;
        };
        let stage_index = usize::from(tank_geometry.area >= water_policy.area_thresholds[0])
            + usize::from(tank_geometry.area >= water_policy.area_thresholds[1]);
        if let Some(stage) = water_policy.tank_stages[stage_index] {
            commands.entity(tank_entity).insert(AudioStage(stage));
        } else {
            commands.entity(tank_entity).remove::<AudioStage>();
        }
    }
}
