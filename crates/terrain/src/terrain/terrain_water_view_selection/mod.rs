use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterRegionFade, TerrainWaterRegionVisibility,
};

pub(super) fn update_camera_terrain_water_membership(
    phase: Res<State<GamePhase>>,
    cameras: Query<(Entity, &Camera), With<ZooCamera>>,
    transforms: TransformHelper,
    indexes: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
    mut view: ResMut<TerrainWaterViewState>,
) {
    let camera = cameras
        .single()
        .ok()
        .filter(|(_, camera)| camera.is_active && *phase.get() == GamePhase::InGame)
        .and_then(|(entity, _)| transforms.compute_global_transform(entity).ok());
    let Some(camera) = camera else {
        view.update_camera(None, None);
        return;
    };
    let position = camera.translation();
    let wet_region = indexes.single().ok().and_then(|index| {
        let asset = assets.get(index.terrain())?;
        let owner = index.owner_at_world_xz(Vec2::new(position.x, position.z), asset)?;
        match owner {
            Some(owner) => Some(Some((owner, regions.get(owner)?.surface_height(asset)?))),
            None => Some(None),
        }
    });
    view.update_camera(Some(position), wet_region);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_terrain_water_view_selection(
    phase: Res<State<GamePhase>>,
    cameras: Query<(Entity, &Camera, &Projection), With<ZooCamera>>,
    transforms: TransformHelper,
    indexes: Query<&TerrainWaterMembershipIndex>,
    mut regions: Query<(
        Entity,
        &TerrainWaterRegionVisibility,
        &mut TerrainWaterRegionFade,
    )>,
    region_sources: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
    mut view: ResMut<TerrainWaterViewState>,
) {
    let Some((entity, _, Projection::Perspective(projection))) = cameras
        .single()
        .ok()
        .filter(|(_, camera, _)| camera.is_active && *phase.get() == GamePhase::InGame)
    else {
        view.update_selection(None, false);
        return;
    };
    let (Ok(camera), Ok(index)) = (
        transforms.compute_global_transform(entity),
        indexes.single(),
    ) else {
        view.update_selection(None, false);
        return;
    };
    let Some(asset) = assets.get(index.terrain()) else {
        view.update_selection(None, false);
        return;
    };
    let config = asset
        .canonical_terrain_grid()
        .presentation
        .water_distance_fade;
    let position = camera.translation();
    let horizontal_fov = 2.0 * ((projection.fov * 0.5).tan() * projection.aspect_ratio).atan();
    let across = *camera.right() * (horizontal_fov * config.search * 0.5).tan();
    let up = *camera.up() * (projection.fov * config.search * 0.5).tan();
    let forward = *camera.forward();
    let mut hits = Vec::new();
    for (owner, source) in region_sources.iter() {
        if source.terrain().id() != index.terrain() {
            continue;
        }
        let Some(presentation) = source
            .biome(asset)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            view.update_selection(None, false);
            return;
        };
        if presentation
            .surface_material
            .configuration
            .scum
            .as_ref()
            .and_then(|scum| scum.image.as_deref())
            .and_then(|image| asset.texture_image(image))
            .is_some()
        {
            continue;
        }
        let Some(height) = source.surface_height(asset) else {
            view.update_selection(None, false);
            return;
        };
        match intersect_water_region_footprint(
            owner, height, position, forward, across, up, index, asset,
        ) {
            Ok(Some(hit)) => hits.push(hit),
            Ok(None) => {}
            Err(()) => {
                view.update_selection(None, false);
                return;
            }
        }
    }
    let previous = view.selected_region();
    let candidate = select_water_region_from_view_hits(
        hits,
        previous,
        config.minimum_distance,
        config.maximum_distance,
    );
    let mut selected = candidate;
    if let Some(previous) = previous {
        if let Ok((_, visibility, mut fade)) = regions.get_mut(previous) {
            if candidate == Some(previous) {
                fade.fade_in(config.time_seconds);
            } else {
                fade.fade_out(config.time_seconds, visibility.0);
                if visibility.0 {
                    selected = Some(previous);
                }
            }
        }
    }
    if selected != previous {
        if let Some(selected) = selected {
            if let Ok((_, _, mut fade)) = regions.get_mut(selected) {
                fade.fade_in(config.time_seconds);
            }
        }
    }
    let height = selected.and_then(|selected| region_sources.get(selected)?.surface_height(asset));
    let alternate = (selected != previous && selected.is_some())
        || regions.iter().any(|(owner, visible, fade)| {
            region_sources
                .get(owner)
                .is_some_and(|source| source.terrain().id() == index.terrain())
                && ((Some(owner) != selected && visible.0)
                    || (Some(owner) == selected && fade.is_transitioning())
                    || (candidate != previous && Some(owner) == previous && visible.0))
        });
    view.update_selection(Some(selected.zip(height)), alternate);
}

#[allow(clippy::too_many_arguments)]
fn intersect_water_region_footprint(
    owner: Entity,
    height: f32,
    position: Vec3,
    forward: Vec3,
    across: Vec3,
    up: Vec3,
    index: &TerrainWaterMembershipIndex,
    terrain: &TerrainAsset,
) -> Result<Option<TerrainWaterViewHit>, ()> {
    for (probe, direction) in [
        forward,
        forward - up - across,
        forward - up + across,
        forward + up - across,
        forward + up + across,
    ]
    .into_iter()
    .enumerate()
    {
        let can_intersect = if height > position.y {
            direction.y > 0.0001
        } else if height < position.y {
            direction.y < -0.0001
        } else {
            direction.y * direction.y > 0.00000001
        };
        if !can_intersect {
            continue;
        }
        let distance = (height - position.y) / direction.y;
        let point = position + direction * distance;
        if index
            .owner_at_world_xz(Vec2::new(point.x, point.z), terrain)
            .ok_or(())?
            == Some(owner)
        {
            return Ok(Some(TerrainWaterViewHit {
                owner,
                distance,
                direct: probe == 0,
            }));
        }
    }
    Ok(None)
}

/// A footprint hit is a transient query result, not another water record.
pub(super) struct TerrainWaterViewHit {
    pub(super) owner: Entity,
    pub(super) distance: f32,
    pub(super) direct: bool,
}

pub(super) fn select_water_region_from_view_hits(
    hits: impl IntoIterator<Item = TerrainWaterViewHit>,
    previous: Option<Entity>,
    minimum_distance: f32,
    maximum_distance: f32,
) -> Option<Entity> {
    let mut selected = None;
    let mut distance = maximum_distance;
    let mut direct_selected = false;
    let mut retain_previous = false;
    for hit in hits {
        if previous == Some(hit.owner) && hit.distance < maximum_distance * 1.1 {
            retain_previous = true;
        }
        if direct_selected {
            if hit.direct && hit.distance < distance {
                selected = Some(hit.owner);
                distance = hit.distance;
            }
        } else if hit.direct {
            if hit.distance < maximum_distance {
                selected = Some(hit.owner);
                distance = hit.distance;
                direct_selected = true;
            }
        } else if hit.distance < distance {
            selected = Some(hit.owner);
            distance = hit.distance;
        }
    }
    if direct_selected {
        let minimum = minimum_distance * if previous.is_none() { 1.1 } else { 1.0 };
        (distance >= minimum).then_some(selected).flatten()
    } else if retain_previous {
        previous
    } else {
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_hit_has_priority_over_closer_corner_and_entry_uses_hysteresis() {
        let near = Entity::from_bits(1 << 32 | 1);
        let direct = Entity::from_bits(1 << 32 | 2);
        assert_eq!(
            select_water_region_from_view_hits(
                [
                    TerrainWaterViewHit {
                        owner: near,
                        distance: 3.0,
                        direct: false
                    },
                    TerrainWaterViewHit {
                        owner: direct,
                        distance: 10.0,
                        direct: true
                    },
                ],
                None,
                2.0,
                35.0
            ),
            Some(direct)
        );
        assert_eq!(
            select_water_region_from_view_hits(
                [TerrainWaterViewHit {
                    owner: direct,
                    distance: 2.1,
                    direct: true
                },],
                None,
                2.0,
                35.0
            ),
            None
        );
        assert_eq!(
            select_water_region_from_view_hits(
                [TerrainWaterViewHit {
                    owner: direct,
                    distance: 2.1,
                    direct: true
                },],
                Some(direct),
                2.0,
                35.0
            ),
            Some(direct)
        );
    }
}
