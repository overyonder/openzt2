use bevy::{camera::ScalingMode, prelude::*};
use openzt2_game_data::world_definitions::camera_definitions::{
    CameraProjectionKind, CameraTuningDefinition,
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use crate::plugins::settings::graphics_settings_types::ThreeLevelGraphicsDetail;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;

use super::{
    super::{
        camera_runtime_state_types::{
            CameraBounds, CameraDefinition, CameraTuning, OverheadRig, ZooCamera,
        },
        fixed_aspect_perspective_projection::FixedAspectPerspectiveProjection,
        math::clamp_rig,
    },
    overhead_camera_zoom_policy::maximum_overhead_camera_zoom_for_graphics_detail,
};

/// Updates projection and movement settings when a mode selects a different camera definition.
pub(in crate::plugins::camera) fn apply_authored_camera_definition_to_runtime_components(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    graphics: Res<GraphicsSettings>,
    roots: Query<&WorldTerrainHorizontalBounds, With<WorldRoot>>,
    mut cameras: Query<
        (
            &CameraDefinition,
            &WorldMember,
            &mut Projection,
            &mut CameraTuning,
            &mut CameraBounds,
            &mut OverheadRig,
        ),
        (With<ZooCamera>, Changed<CameraDefinition>),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (selected, member, mut projection, mut tuning, mut bounds, mut rig) in &mut cameras {
        let Ok(world_bounds) = roots.get(member.root) else {
            continue;
        };
        let Some(definition) = definitions.find_camera(selected.0) else {
            continue;
        };
        let Some((next_projection, next_tuning, _, next_bounds)) =
            camera_runtime_components_from_authored_definition(
                definition,
                world_bounds,
                graphics.max_overhead_zoom,
            )
        else {
            continue;
        };
        *projection = next_projection;
        *tuning = next_tuning;
        *bounds = next_bounds;
        clamp_rig(&mut rig, &tuning, &bounds);
    }
}

pub(super) fn camera_runtime_components_from_authored_definition(
    definition: &CameraTuningDefinition,
    world: &WorldTerrainHorizontalBounds,
    maximum_zoom_detail: ThreeLevelGraphicsDetail,
) -> Option<(Projection, CameraTuning, OverheadRig, CameraBounds)> {
    let near = f32::from(definition.near_m);
    let far = f32::from(definition.far_m);
    let fov = f32::from(definition.fov_y_radians);
    let initial_zoom = f32::from(definition.initial_zoom_m);
    let projection = match definition.projection {
        CameraProjectionKind::Perspective => {
            if !(0.0..std::f32::consts::PI).contains(&fov) {
                return None;
            }
            let aspect_ratio = f32::from(definition.fixed_aspect_ratio);
            if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
                return None;
            }
            Projection::custom(FixedAspectPerspectiveProjection::new(
                fov,
                aspect_ratio,
                near,
                far,
            ))
        }
        CameraProjectionKind::Orthographic => {
            let vertical_view_per_zoom = f32::from(definition.vertical_view_per_zoom);
            if !vertical_view_per_zoom.is_finite() || vertical_view_per_zoom <= 0.0 {
                return None;
            }
            let mut projection = OrthographicProjection::default_3d();
            projection.near = near;
            projection.far = far;
            projection.scaling_mode = ScalingMode::FixedVertical {
                viewport_height: initial_zoom * vertical_view_per_zoom,
            };
            Projection::Orthographic(projection)
        }
    };
    let minimum_zoom = f32::from(definition.minimum_zoom_m);
    let maximum_distance_by_detail = definition.maximum_zoom_m.map(f32::from);
    let maximum_zoom = maximum_overhead_camera_zoom_for_graphics_detail(
        maximum_distance_by_detail,
        maximum_zoom_detail,
    );
    let tuning = CameraTuning {
        distance: minimum_zoom..=maximum_zoom,
        maximum_distance_by_detail,
        vertical_view_per_zoom: f32::from(definition.vertical_view_per_zoom),
        pitch: f32::from(definition.pitch_radians[0])..=f32::from(definition.pitch_radians[1]),
        yaw: f32::from(definition.yaw_radians[0])..=f32::from(definition.yaw_radians[1]),
        pan_speed_mps: f32::from(definition.pan_speed_mps),
        turn_speed_rps: f32::from(definition.turn_speed_rps),
        zoom_speed_mps: f32::from(definition.zoom_speed_mps),
        pan_start_rate: f32::from(definition.pan_start_rate),
        pan_stop_rate: f32::from(definition.pan_stop_rate),
        soft_fit_distance_m: f32::from(definition.soft_fit_distance_m),
        soft_fit_speed_mps: f32::from(definition.soft_fit_speed_mps),
        no_tilt_on_fit: definition.no_tilt_on_fit,
        collision_radius_m: f32::from(definition.collision_radius_cm) * 0.01,
        parenting_offset_m: f32::from(definition.parenting_offset_m),
        edge_scroll: definition.edge_scroll,
        zoom_speed_samples: std::array::from_fn(|index| {
            Vec2::new(
                f32::from(definition.zoom_speed_samples[index][0]),
                f32::from(definition.zoom_speed_samples[index][1]),
            )
        }),
        zoom_speed_sample_count: definition.zoom_speed_sample_count,
        sub_zero_zoom_multiplier: f32::from(definition.sub_zero_zoom_multiplier),
    };
    let source_offset = Vec3::new(
        f32::from(definition.offset_m[0]),
        f32::from(definition.offset_m[1]),
        f32::from(definition.offset_m[2]),
    );
    let bounds = CameraBounds {
        min: world.min,
        max: world.max,
        camera_clearance_m: f32::from(definition.ground_buffer_m)
            + f32::from(definition.collision_radius_cm) * 0.01,
    };
    let rig = OverheadRig {
        focus: (world.min + world.max) * 0.5 + source_offset.xz(),
        height_m: 0.0,
        look_at_height_m: source_offset.y,
        yaw: (f32::from(definition.yaw_radians[0]) + f32::from(definition.yaw_radians[1])) * 0.5,
        pitch: f32::from(definition.initial_pitch_radians),
        distance: initial_zoom,
        look_at_distance_m: f32::from(definition.look_at_distance_m),
        minimum_zoom_offset_m: 0.0,
        camera_ground_fit_offset_m: 0.0,
        target_ground_fit_offset_m: 0.0,
        pan_accumulators: [0.0; 4],
    };
    (tuning.is_valid()
        && bounds.is_valid()
        && rig.focus.is_finite()
        && rig.yaw.is_finite()
        && rig.pitch.is_finite()
        && rig.distance.is_finite()
        && source_offset.is_finite())
    .then_some((projection, tuning, rig, bounds))
}
