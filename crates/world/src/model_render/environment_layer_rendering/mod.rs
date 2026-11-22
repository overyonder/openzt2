use std::collections::BTreeMap;

use bevy::camera::{
    visibility::RenderLayers, Camera3dDepthLoadOp, CameraOutputMode, CameraUpdateSystems, Exposure,
    Hdr, RenderTarget,
};
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::view::{Msaa, Tonemapping};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::{
    model_render::environment_render_layer::EnvironmentRenderLayer,
    world_spawn::world_membership_types::WorldMember,
};

pub(super) const FIRST_ENVIRONMENT_RENDER_LAYER: usize = 64;

#[derive(Component)]
struct EnvironmentStackCamera(Option<u16>);

#[derive(Component)]
struct EnvironmentWorldCameraState {
    color: ClearColorConfig,
    depth: Camera3dDepthLoadOp,
}

pub(super) fn register_inherited_environment_render_layers(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (
            route_authored_environment_layers
                .before(super::model_scene_visibility_and_shadow_projection::publish_model_draw_shadow_policy_sources),
            synchronize_environment_camera_stack
                .before(super::active_model_view_cameras::refresh_active_model_view_cameras)
                .before(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        ),
    );
}

fn environment_render_mask(layer: u16) -> RenderLayers {
    RenderLayers::none().with(FIRST_ENVIRONMENT_RENDER_LAYER + usize::from(layer))
}

fn route_authored_environment_layers(
    mut commands: Commands,
    mut sources: Query<
        (Entity, &EnvironmentRenderLayer, Option<&mut RenderLayers>),
        Changed<EnvironmentRenderLayer>,
    >,
    mut removed: RemovedComponents<EnvironmentRenderLayer>,
) {
    for (entity, layer, layers) in &mut sources {
        let mask = environment_render_mask(layer.layer);
        if let Some(mut layers) = layers {
            layers.set_if_neq(mask);
        } else {
            commands.entity(entity).insert(mask);
        }
    }
    for entity in removed.read().filter(|&entity| !sources.contains(entity)) {
        commands.entity(entity).try_remove::<RenderLayers>();
    }
}

#[allow(clippy::type_complexity)]
fn synchronize_environment_camera_stack(
    mut commands: Commands,
    layers: Query<(&EnvironmentRenderLayer, &WorldMember, &Visibility)>,
    mut world_cameras: Query<
        (
            Entity,
            &WorldMember,
            &mut Camera,
            &mut Camera3d,
            Ref<Projection>,
            Ref<RenderTarget>,
            &Exposure,
            &Msaa,
            Has<Hdr>,
            Option<Ref<DistanceFog>>,
            Option<&EnvironmentWorldCameraState>,
            Option<&Children>,
        ),
        (With<ZooCamera>, Without<EnvironmentStackCamera>),
    >,
    mut stacked_cameras: Query<
        (
            &EnvironmentStackCamera,
            &mut Camera,
            &mut Camera3d,
            &mut Projection,
            &mut RenderTarget,
            &mut Exposure,
            &mut Msaa,
            Has<Hdr>,
            Option<&mut DistanceFog>,
        ),
        Without<ZooCamera>,
    >,
) {
    for (
        world_camera,
        member,
        mut view,
        mut view_3d,
        projection,
        target,
        exposure,
        msaa,
        hdr,
        fog,
        previous,
        children,
    ) in &mut world_cameras
    {
        let mut authored_layers = BTreeMap::new();
        for (layer, owner, visibility) in &layers {
            if owner.root == member.root && *visibility != Visibility::Hidden {
                *authored_layers.entry(layer.layer).or_insert(false) |= layer.clear_depth_after;
            }
        }
        let mut existing = BTreeMap::new();
        for child in children.into_iter().flatten() {
            if let Ok((layer, ..)) = stacked_cameras.get(*child) {
                existing.insert(layer.0, *child);
            }
        }
        if authored_layers.is_empty() {
            for child in existing.into_values() {
                commands.entity(child).despawn();
            }
            if let Some(previous) = previous {
                view.clear_color = previous.color;
                view_3d.depth_load_op = previous.depth.clone();
                commands
                    .entity(world_camera)
                    .remove::<EnvironmentWorldCameraState>();
            }
            continue;
        }
        let background = previous.map_or(view.clear_color, |previous| previous.color);
        if previous.is_none() {
            commands
                .entity(world_camera)
                .insert(EnvironmentWorldCameraState {
                    color: background,
                    depth: view_3d.depth_load_op.clone(),
                });
        }
        if !matches!(view.clear_color, ClearColorConfig::None) {
            view.clear_color = ClearColorConfig::None;
        }
        let mut clear_depth = true;
        let first_order =
            view.order - isize::try_from(authored_layers.len() + 1).expect("u16 layer count");
        // The empty first view clears the shared color target before the
        // authored views load and composite into it.
        for (ordinal, (layer, clear_depth_after)) in std::iter::once((None, true))
            .chain(
                authored_layers
                    .into_iter()
                    .map(|(layer, clear)| (Some(layer), clear)),
            )
            .enumerate()
        {
            let order = first_order + isize::try_from(ordinal).expect("u16 layer index");
            let color = if layer.is_some() {
                ClearColorConfig::None
            } else {
                background
            };
            let mut camera_entity;
            if let Some(child) = existing.remove(&layer) {
                let Ok((
                    _,
                    mut camera,
                    mut camera_3d,
                    mut child_projection,
                    mut child_target,
                    mut child_exposure,
                    mut child_msaa,
                    child_hdr,
                    child_fog,
                )) = stacked_cameras.get_mut(child)
                else {
                    continue;
                };
                if view.is_changed() || camera.order != order {
                    camera.order = order;
                    camera.viewport = view.viewport.clone();
                    camera.is_active = view.is_active;
                    camera.sub_camera_view = view.sub_camera_view;
                }
                set_depth_boundary(&mut camera_3d, clear_depth);
                if projection.is_changed() {
                    *child_projection = (*projection).clone();
                }
                if target.is_changed() {
                    *child_target = (*target).clone();
                }
                if child_exposure.ev100.to_bits() != exposure.ev100.to_bits() {
                    *child_exposure = *exposure;
                }
                child_msaa.set_if_neq(*msaa);
                camera_entity = commands.entity(child);
                match (fog.as_ref(), child_fog) {
                    (Some(fog), Some(mut current)) if fog.is_changed() => {
                        *current = (**fog).clone()
                    }
                    (Some(fog), None) => {
                        camera_entity.insert((**fog).clone());
                    }
                    (None, Some(_)) => {
                        camera_entity.remove::<DistanceFog>();
                    }
                    _ => {}
                }
                if hdr && !child_hdr {
                    camera_entity.insert(Hdr);
                } else if !hdr && child_hdr {
                    camera_entity.remove::<Hdr>();
                }
            } else {
                camera_entity = commands.spawn((
                    EnvironmentStackCamera(layer),
                    Camera {
                        order,
                        clear_color: color,
                        output_mode: CameraOutputMode::Skip,
                        viewport: view.viewport.clone(),
                        is_active: view.is_active,
                        sub_camera_view: view.sub_camera_view,
                        ..default()
                    },
                    Camera3d {
                        depth_load_op: depth_load_operation(clear_depth),
                        depth_texture_usages: view_3d.depth_texture_usages,
                    },
                    Transform::IDENTITY,
                    (*projection).clone(),
                    (*target).clone(),
                    *exposure,
                    *msaa,
                    Tonemapping::None,
                    layer.map_or_else(RenderLayers::none, environment_render_mask),
                    ChildOf(world_camera),
                ));
                if hdr {
                    camera_entity.insert(Hdr);
                }
                if let Some(fog) = fog.as_ref() {
                    camera_entity.insert((**fog).clone());
                }
            }
            clear_depth = clear_depth_after;
        }
        set_depth_boundary(&mut view_3d, clear_depth);
        for child in existing.into_values() {
            commands.entity(child).despawn();
        }
    }
}

fn depth_load_operation(clear: bool) -> Camera3dDepthLoadOp {
    if clear {
        Camera3dDepthLoadOp::Clear(0.0)
    } else {
        Camera3dDepthLoadOp::Load
    }
}

fn set_depth_boundary(camera: &mut Mut<Camera3d>, clear: bool) {
    if matches!(camera.depth_load_op, Camera3dDepthLoadOp::Clear(_)) != clear {
        camera.depth_load_op = depth_load_operation(clear);
    }
}
