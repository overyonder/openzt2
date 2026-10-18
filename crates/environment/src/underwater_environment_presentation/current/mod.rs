use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::{
    environment::environment_state_types::{AuthoredWindShaderPresentationState, WorldEnvironment},
    terrain::terrain_water_presentation_types::{
        TerrainWaterRegionEdit, TerrainWaterRegionQuery, TerrainWaterRegionRetired,
        TerrainWaterRegionSource,
    },
};

pub(crate) fn update_water_region_currents(
    mut commands: Commands,
    time: Res<Time<Virtual>>,
    terrains: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut currents: Query<
        (Entity, &mut AuthoredWindShaderPresentationState),
        (
            Without<WorldEnvironment>,
            Or<(
                With<TerrainWaterRegionSource>,
                With<TerrainWaterRegionEdit>,
                With<TerrainWaterRegionRetired>,
            )>,
        ),
    >,
    mut random: ResMut<NativeVisualRandomStream>,
) {
    for (entity, _) in &currents {
        if regions.get(entity).is_none() {
            commands
                .entity(entity)
                .remove::<AuthoredWindShaderPresentationState>();
        }
    }
    for (entity, region) in regions.iter() {
        let configuration = (|| {
            let terrain = terrains.get(region.terrain())?;
            let current = region
                .biome(terrain)?
                .water_presentation
                .as_ref()?
                .surface_material
                .configuration
                .current
                .as_ref()?;
            Some((current.wind_speed?, current.gustiness?))
        })();
        let Some((speed, gustiness)) = configuration else {
            commands
                .entity(entity)
                .remove::<AuthoredWindShaderPresentationState>();
            continue;
        };
        if let Ok((_, mut current)) = currents.get_mut(entity) {
            current.advance_current(speed, gustiness, time.delta_secs(), || random.unit_f32());
        } else {
            let mut current = AuthoredWindShaderPresentationState::from_current(speed, gustiness);
            current.advance_current(speed, gustiness, time.delta_secs(), || random.unit_f32());
            commands.entity(entity).insert(current);
        }
    }
}
