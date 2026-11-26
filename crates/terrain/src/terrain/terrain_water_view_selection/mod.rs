use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionRetired, TerrainWaterRegionSource,
};
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterCameraContact, TerrainWaterCameraPose, TerrainWaterCaptureRegion,
    TerrainWaterCaptureSelection, TerrainWaterRegionFade, TerrainWaterRegionVisibility,
};

pub(super) fn sample_terrain_water_camera_pose(
    phase: Res<State<GamePhase>>,
    mut cameras: Query<(Entity, &Camera, &mut TerrainWaterCameraPose), With<ZooCamera>>,
    transforms: TransformHelper,
) {
    let Ok((entity, camera, mut pose)) = cameras.single_mut() else {
        return;
    };
    let transform = if camera.is_active && *phase.get() == GamePhase::InGame {
        transforms.compute_global_transform(entity).ok()
    } else {
        None
    };
    pose.set_if_neq(TerrainWaterCameraPose(transform));
}

type ChangedWaterRegion = Or<(
    Changed<TerrainWaterRegionSource>,
    Changed<TerrainWaterRegionEdit>,
    Added<TerrainWaterRegionRetired>,
)>;

pub(super) fn update_camera_terrain_water_membership(
    mut cameras: Query<
        (Ref<TerrainWaterCameraPose>, &mut TerrainWaterCameraContact),
        With<ZooCamera>,
    >,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    changed_regions: Query<(), ChangedWaterRegion>,
    (mut removed_edits, mut removed_sources, mut restored_regions): (
        RemovedComponents<TerrainWaterRegionEdit>,
        RemovedComponents<TerrainWaterRegionSource>,
        RemovedComponents<TerrainWaterRegionRetired>,
    ),
    regions: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
) {
    let geometry_changed = !changed_regions.is_empty()
        | (removed_edits.read().count() != 0)
        | (removed_sources.read().count() != 0)
        | (restored_regions.read().count() != 0);
    let Ok((pose, mut contact)) = cameras.single_mut() else {
        return;
    };
    let Some(camera) = pose.0 else {
        contact.set_if_neq(TerrainWaterCameraContact::default());
        return;
    };
    let Ok(index) = indexes.single() else {
        contact.set_if_neq(TerrainWaterCameraContact::from_membership(
            camera.translation(),
            None,
        ));
        return;
    };
    if !(pose.is_changed() || index.is_changed() || geometry_changed || assets.is_changed()) {
        return;
    }
    let position = camera.translation();
    let wet_region = assets.get(index.terrain()).and_then(|asset| {
        let owner = index.owner_at_world_xz(position.xz(), asset)?;
        match owner {
            Some(owner) => Some(Some((owner, regions.get(owner)?.surface_height(asset)?))),
            None => Some(None),
        }
    });
    contact.set_if_neq(TerrainWaterCameraContact::from_membership(
        position, wet_region,
    ));
}

#[derive(Component, Default)]
pub(super) struct TerrainWaterCaptureHits(Option<Vec<TerrainWaterViewHit>>);

pub(super) fn update_terrain_water_capture_hits(
    mut cameras: Query<
        (
            Ref<Projection>,
            Ref<TerrainWaterCameraPose>,
            &mut TerrainWaterCaptureHits,
        ),
        With<ZooCamera>,
    >,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    changed_regions: Query<(), ChangedWaterRegion>,
    (mut removed_edits, mut removed_sources, mut restored_regions): (
        RemovedComponents<TerrainWaterRegionEdit>,
        RemovedComponents<TerrainWaterRegionSource>,
        RemovedComponents<TerrainWaterRegionRetired>,
    ),
    region_sources: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
) {
    let geometry_changed = !changed_regions.is_empty()
        | (removed_edits.read().count() != 0)
        | (removed_sources.read().count() != 0)
        | (restored_regions.read().count() != 0);
    let Ok((projection, pose, mut hits)) = cameras.single_mut() else {
        return;
    };
    let (Projection::Perspective(perspective), Some(camera), Ok(index)) =
        (&*projection, pose.0, indexes.single())
    else {
        if hits.0.is_some() {
            hits.0 = None;
        }
        return;
    };
    if !(projection.is_changed()
        || pose.is_changed()
        || index.is_changed()
        || geometry_changed
        || assets.is_changed())
    {
        return;
    }
    hits.0 = assets.get(index.terrain()).and_then(|asset| {
        collect_water_capture_hits(perspective, camera, &index, asset, &region_sources)
    });
}

fn collect_water_capture_hits(
    projection: &PerspectiveProjection,
    camera: GlobalTransform,
    index: &TerrainWaterMembershipIndex,
    asset: &TerrainAsset,
    region_sources: &TerrainWaterRegionQuery,
) -> Option<Vec<TerrainWaterViewHit>> {
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
            return None;
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
            return None;
        };
        match intersect_water_region_footprint(
            owner, height, position, forward, across, up, index, asset,
        ) {
            Ok(Some(hit)) => hits.push(hit),
            Ok(None) => {}
            Err(()) => {
                return None;
            }
        }
    }
    Some(hits)
}

pub(super) fn update_terrain_water_view_selection(
    mut commands: Commands,
    mut cameras: Query<
        (
            Entity,
            Ref<TerrainWaterCaptureHits>,
            &mut TerrainWaterCaptureSelection,
            Option<Ref<TerrainWaterCaptureRegion>>,
        ),
        With<ZooCamera>,
    >,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    mut regions: Query<(
        Entity,
        &TerrainWaterRegionVisibility,
        &mut TerrainWaterRegionFade,
    )>,
    region_sources: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
    changed_regions: Query<
        (),
        Or<(
            Changed<TerrainWaterRegionVisibility>,
            Added<TerrainWaterRegionFade>,
            ChangedWaterRegion,
        )>,
    >,
    mut removed_capture: RemovedComponents<TerrainWaterCaptureRegion>,
    mut removed_visibility: RemovedComponents<TerrainWaterRegionVisibility>,
    mut fade_transitions: MessageReader<
        super::terrain_water_view_transitions::WaterFadeTransitionChanged,
    >,
) {
    let Ok((camera, hits, mut selection, region)) = cameras.single_mut() else {
        return;
    };
    let index = indexes.single().ok();
    let capture_removed = removed_capture
        .read()
        .filter(|&entity| entity == camera)
        .count()
        != 0;
    let visibility_removed = removed_visibility.read().count() != 0;
    let fade_changed = fade_transitions.read().count() != 0;
    if !hits.is_changed()
        && !selection.is_added()
        && !region.as_ref().is_some_and(DetectChanges::is_changed)
        && !index.as_ref().is_some_and(DetectChanges::is_changed)
        && !assets.is_changed()
        && changed_regions.is_empty()
        && !capture_removed
        && !visibility_removed
        && !fade_changed
    {
        return;
    }
    let Some((hits, (index, asset))) = hits
        .0
        .as_ref()
        .zip(index.and_then(|index| assets.get(index.terrain()).map(|asset| (index, asset))))
    else {
        selection.set_if_neq(TerrainWaterCaptureSelection::default());
        if region.is_some() {
            commands
                .entity(camera)
                .remove::<TerrainWaterCaptureRegion>();
        }
        return;
    };
    let config = asset
        .canonical_terrain_grid()
        .presentation
        .water_distance_fade;
    let previous = region.map(|region| region.0);
    let candidate = select_water_region_from_view_hits(
        hits.iter().copied(),
        previous,
        config.minimum_distance,
        config.maximum_distance,
    );
    let mut selected = candidate;
    if let Some(previous) = previous {
        if let Ok((_, visibility, mut fade)) = regions.get_mut(previous) {
            let mut next = *fade;
            if candidate == Some(previous) {
                next.fade_in(config.time_seconds);
            } else {
                next.fade_out(config.time_seconds, visibility.0);
                if visibility.0 {
                    selected = Some(previous);
                }
            }
            fade.set_if_neq(next);
        }
    }
    if selected != previous {
        if let Some(selected) = selected {
            if let Ok((_, _, mut fade)) = regions.get_mut(selected) {
                let mut next = *fade;
                next.fade_in(config.time_seconds);
                fade.set_if_neq(next);
            }
        }
    }
    let alternate = (selected != previous && selected.is_some())
        || regions.iter().any(|(owner, visible, fade)| {
            region_sources
                .get(owner)
                .is_some_and(|source| source.terrain().id() == index.terrain())
                && ((Some(owner) != selected && visible.0)
                    || (Some(owner) == selected && fade.is_transitioning())
                    || (candidate != previous && Some(owner) == previous && visible.0))
        });
    selection.set_if_neq(TerrainWaterCaptureSelection {
        resolved: true,
        alternate_regions_visible: alternate,
    });
    let next = selected.filter(|&owner| {
        region_sources
            .get(owner)
            .and_then(|source| source.surface_height(asset))
            .is_some()
    });
    if previous != next {
        if let Some(owner) = next {
            commands
                .entity(camera)
                .insert(TerrainWaterCaptureRegion(owner));
        } else {
            commands
                .entity(camera)
                .remove::<TerrainWaterCaptureRegion>();
        }
    }
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
#[derive(Clone, Copy)]
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
