use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_zoo_state::locomotion::locomotion_types::ActiveTerrainDerivedNavigationGraph;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

use super::guest_view_navigation_marker_types::{
    GuestViewNavigationMarkerOf, GuestViewNavigationMarkerPresentation, GuestViewNavigationMarkers,
};
use super::immersive_mode_state_types::ActiveImmersiveMode;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::GuestViewControl;
use crate::immersive_modes::immersive_control_execution_types::GuestViewTileCount;

const MAXIMUM_GUEST_VIEW_NAVIGATION_MARKER_COUNT: usize = 4;

/// Rebuilds the original guest-view tile markers from the terrain-derived
/// navigation topology when the picked target or navigation graph changes.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_nearest_guest_navigation_positions_as_view_markers(
    pointer_ray: Res<WorldPointerRay>,
    spatial_query: SpatialQuery,
    active_navigation: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    mut active: Query<
        (Entity, &ActiveImmersiveMode, &mut GuestViewTileCount),
        With<GuestViewControl>,
    >,
    parents: Query<&ChildOf>,
    transforms: Query<&GlobalTransform>,
    marker_presentations: Query<&GuestViewNavigationMarkerPresentation>,
    markers: Query<&GuestViewNavigationMarkers>,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    let (Ok((controller_entity, mode, mut count)), Some(ray), Some(active_navigation)) =
        (active.single_mut(), pointer_ray.0, active_navigation)
    else {
        return;
    };
    if mode.mode != ImmersiveMode::GuestView {
        return;
    }
    let Some(hit) = spatial_query.cast_ray_predicate(
        ray.origin,
        ray.direction,
        f32::MAX,
        false,
        &SpatialQueryFilter::DEFAULT,
        &|entity| {
            entity != controller_entity
                && mode.subject.is_none_or(|subject| {
                    entity != subject && !entity_has_ancestor(entity, subject, &parents)
                })
        },
    ) else {
        despawn_projected_navigation_markers(markers.get(controller_entity).ok(), &mut commands);
        count.0 = 0;
        return;
    };
    let Ok(target_transform) = transforms.get(hit.entity) else {
        return;
    };
    let existing = markers.get(controller_entity).ok();
    if !active_navigation.is_changed()
        && existing
            .into_iter()
            .flat_map(|markers| markers.iter())
            .any(|entity| {
                marker_presentations
                    .get(entity)
                    .is_ok_and(|presentation| presentation.targeted_entity == hit.entity)
            })
    {
        return;
    }
    despawn_projected_navigation_markers(existing, &mut commands);
    let world = target_transform.translation();
    let position_cm = (world * 100.0).round().as_ivec3().to_array();
    let mut nearest_navigation_positions =
        [(f32::INFINITY, Vec3::ZERO); MAXIMUM_GUEST_VIEW_NAVIGATION_MARKER_COUNT];
    for position in active_navigation.candidate_positions(position_cm, NavFlags::GUEST) {
        let position = Vec3::new(position[0] as f32, position[1] as f32, position[2] as f32) * 0.01;
        insert_navigation_position_among_nearest_guest_view_positions(
            &mut nearest_navigation_positions,
            (position.xz() - world.xz()).length_squared(),
            position,
        );
    }
    let marker_count = nearest_navigation_positions
        .iter()
        .take_while(|(distance, _)| distance.is_finite())
        .count();
    count.0 = marker_count as u32;
    if marker_count == 0 {
        return;
    }
    let mut asset = GizmoAsset::new();
    for &(_, position) in &nearest_navigation_positions[..marker_count] {
        let center = position + Vec3::Y * 0.08;
        let radius = 0.28;
        asset.line(
            center - Vec3::X * radius,
            center + Vec3::X * radius,
            Color::srgba(1.0, 0.85, 0.1, 0.8),
        );
        asset.line(
            center - Vec3::Z * radius,
            center + Vec3::Z * radius,
            Color::srgba(1.0, 0.85, 0.1, 0.8),
        );
        asset.line(
            center,
            center + Vec3::Y * 0.8,
            Color::srgba(1.0, 0.85, 0.1, 0.55),
        );
    }
    let asset = gizmos.add(asset);
    commands.spawn((
        GuestViewNavigationMarkerOf(controller_entity),
        GuestViewNavigationMarkerPresentation {
            targeted_entity: hit.entity,
        },
        Transform::IDENTITY,
        Visibility::Visible,
        Gizmo {
            handle: asset,
            line_config: GizmoLineConfig {
                width: 3.0,
                ..default()
            },
            depth_bias: -0.001,
        },
    ));
}

fn entity_has_ancestor(mut entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    while let Ok(parent) = parents.get(entity) {
        entity = parent.parent();
        if entity == ancestor {
            return true;
        }
    }
    false
}

pub(super) fn drop_navigation_markers_when_guest_view_ends(
    removed: On<Remove<GuestViewControl>>,
    markers: Query<&GuestViewNavigationMarkers>,
    mut commands: Commands,
) {
    despawn_projected_navigation_markers(markers.get(removed.entity).ok(), &mut commands);
}

fn despawn_projected_navigation_markers(
    markers: Option<&GuestViewNavigationMarkers>,
    commands: &mut Commands,
) {
    for marker in markers.into_iter().flat_map(|markers| markers.iter()) {
        commands.entity(marker).try_despawn();
    }
}

pub(super) fn insert_navigation_position_among_nearest_guest_view_positions(
    nearest_navigation_positions: &mut [(f32, Vec3); MAXIMUM_GUEST_VIEW_NAVIGATION_MARKER_COUNT],
    squared_distance_from_target: f32,
    navigation_position: Vec3,
) {
    let insertion_index =
        nearest_navigation_positions.partition_point(|(candidate_squared_distance, _)| {
            *candidate_squared_distance <= squared_distance_from_target
        });
    if insertion_index >= nearest_navigation_positions.len() {
        return;
    }
    let last_index = nearest_navigation_positions.len() - 1;
    nearest_navigation_positions.copy_within(insertion_index..last_index, insertion_index + 1);
    nearest_navigation_positions[insertion_index] =
        (squared_distance_from_target, navigation_position);
}
