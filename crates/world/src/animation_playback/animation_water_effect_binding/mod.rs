use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::{
    effect::particle_plane_binding::ParticleEffectPlane,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};

#[derive(Component, Clone, Copy)]
pub(super) struct AnimationWaterEffect {
    pub(super) region: Entity,
    pub(super) target: Option<Entity>,
    pub(super) offset: f32,
}

impl AnimationWaterEffect {
    pub(super) fn plane(self, height: f32) -> ParticleEffectPlane {
        let normal = if self.offset > 0.0 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        };
        ParticleEffectPlane(normal.extend(height + self.offset))
    }
}

pub(super) fn resolve_water_effect_region(
    position: Vec3,
    indexes: &Query<&TerrainWaterMembershipIndex>,
    regions: &TerrainWaterRegionQuery,
    terrains: &Assets<TerrainAsset>,
) -> Option<Entity> {
    indexes.iter().find_map(|index| {
        let terrain = terrains.get(index.terrain())?;
        let owner = index.owner_at_world_xz(position.xz(), terrain)??;
        (regions.get(owner)?.terrain().id() == index.terrain()).then_some(owner)
    })
}

pub(super) fn update_animation_water_effect_planes(
    mut commands: Commands,
    transforms: TransformHelper,
    effects: Query<(Entity, &AnimationWaterEffect)>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
) {
    for (entity, binding) in &effects {
        let plane = (|| {
            let position = transforms
                .compute_global_transform(binding.target.unwrap_or(entity))
                .ok()?
                .translation();
            if resolve_water_effect_region(position, &indexes, &regions, &terrains)?
                != binding.region
            {
                return None;
            }
            let source = regions.get(binding.region)?;
            Some(binding.plane(source.surface_height(terrains.get(source.terrain())?)?))
        })();
        if let Some(plane) = plane {
            commands.entity(entity).insert(plane);
        } else {
            commands.entity(entity).despawn();
        }
    }
}
