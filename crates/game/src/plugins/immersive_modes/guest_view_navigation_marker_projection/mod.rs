use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::plugins::{
    camera::world_pointer_ray_types::WorldPointerRay,
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, NavFlags, NavigationOverlay,
    },
};

use super::guest_view_navigation_marker_types::GuestViewNavigationMarkerPresentation;
use super::{
    immersive_mode_control_types::{GuestViewControl, GuestViewTileCount},
    immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
};

const MAXIMUM_GUEST_VIEW_NAVIGATION_MARKER_COUNT: usize = 4;

/// Rebuilds the original guest-view tile markers from the terrain-derived
/// navigation topology when the picked target or sparse closure overlay
/// changes. Only the selected entity and asset handle are retained.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_nearest_guest_navigation_positions_as_view_markers(
    pointer_ray: Res<WorldPointerRay>,
    spatial_query: SpatialQuery,
    active_navigation: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    overlay: Option<Res<NavigationOverlay>>,
    mut active: Query<
        (Entity, &ActiveImmersiveMode, &mut GuestViewTileCount),
        With<GuestViewControl>,
    >,
    parents: Query<&ChildOf>,
    transforms: Query<&GlobalTransform>,
    marker_presentations: Query<(Entity, &GuestViewNavigationMarkerPresentation)>,
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
        retire_navigation_marker_presentations_for_guest_view_controller(
            controller_entity,
            &marker_presentations,
            &mut gizmos,
            &mut commands,
        );
        count.0 = 0;
        return;
    };
    let Ok(target_transform) = transforms.get(hit.entity) else {
        return;
    };
    let revision = overlay.as_deref().map_or(0, |overlay| overlay.revision);
    if marker_presentations.iter().any(|(_, presentation)| {
        presentation.controller_entity == controller_entity
            && presentation.targeted_entity == hit.entity
            && presentation.navigation_revision == revision
            && !active_navigation.is_changed()
    }) {
        return;
    }
    retire_navigation_marker_presentations_for_guest_view_controller(
        controller_entity,
        &marker_presentations,
        &mut gizmos,
        &mut commands,
    );
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
        GuestViewNavigationMarkerPresentation {
            controller_entity,
            targeted_entity: hit.entity,
            navigation_revision: revision,
            gizmo_asset: asset.clone(),
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

pub(super) fn retire_navigation_marker_presentations_without_guest_view_controllers(
    guest_view_controllers: Query<(), With<GuestViewControl>>,
    marker_presentations: Query<(Entity, &GuestViewNavigationMarkerPresentation)>,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    for (entity, presentation) in &marker_presentations {
        if guest_view_controllers
            .get(presentation.controller_entity)
            .is_err()
        {
            gizmos.remove(presentation.gizmo_asset.id());
            commands.entity(entity).despawn();
        }
    }
}

fn retire_navigation_marker_presentations_for_guest_view_controller(
    controller_entity: Entity,
    marker_presentations: &Query<(Entity, &GuestViewNavigationMarkerPresentation)>,
    gizmos: &mut Assets<GizmoAsset>,
    commands: &mut Commands,
) {
    for (entity, presentation) in marker_presentations
        .iter()
        .filter(|(_, presentation)| presentation.controller_entity == controller_entity)
    {
        gizmos.remove(presentation.gizmo_asset.id());
        commands.entity(entity).despawn();
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
