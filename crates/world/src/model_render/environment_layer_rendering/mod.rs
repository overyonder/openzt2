use std::collections::BTreeMap;

use bevy::camera::{
    visibility::RenderLayers, Camera3dDepthLoadOp, CameraOutputMode, CameraUpdateSystems, Exposure,
    Hdr, RenderTarget,
};
use bevy::ecs::{entity::EntityHashSet, relationship::RelationshipTarget};
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::view::{Msaa, Tonemapping};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::{
    model_render::environment_render_layer::EnvironmentRenderLayer,
    world_spawn::world_membership_types::{WorldMember, WorldMembers},
};

pub(super) const FIRST_ENVIRONMENT_RENDER_LAYER: usize = 64;

#[derive(Component)]
struct EnvironmentStackCamera {
    layer: Option<u16>,
    order_offset: isize,
}

#[derive(Component)]
struct EnvironmentWorldCameraState {
    color: ClearColorConfig,
    depth: Camera3dDepthLoadOp,
}

#[derive(Component, PartialEq, Eq, Default)]
struct EnvironmentWorldLayers(BTreeMap<u16, bool>);

#[derive(Message)]
struct EnvironmentLayerMembershipChanged(Entity);

pub(super) fn register_inherited_environment_render_layers(app: &mut App) {
    app.add_message::<EnvironmentLayerMembershipChanged>()
    .add_observer(environment_layer_membership_added)
    .add_observer(environment_layer_membership_discarded)
    .add_systems(
        PostUpdate,
        (
            route_authored_environment_layers
                .before(super::model_scene_visibility_and_shadow_projection::publish_model_draw_shadow_policy_sources),
            project_environment_world_layers.before(synchronize_environment_camera_stack),
            synchronize_environment_camera_stack
                .before(synchronize_environment_camera_optics),
            synchronize_environment_camera_optics
                .before(super::active_model_view_cameras::publish_model_camera_view_inputs)
                .before(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        ),
    );
}

fn environment_layer_membership_added(
    inserted: On<Insert<(EnvironmentRenderLayer, WorldMember)>>,
    layers: Query<&WorldMember, With<EnvironmentRenderLayer>>,
    mut changed: MessageWriter<EnvironmentLayerMembershipChanged>,
) {
    if let Ok(member) = layers.get(inserted.entity) {
        changed.write(EnvironmentLayerMembershipChanged(member.root));
    }
}

fn environment_layer_membership_discarded(
    discarded: On<Discard<(EnvironmentRenderLayer, WorldMember)>>,
    layers: Query<&WorldMember, With<EnvironmentRenderLayer>>,
    mut changed: MessageWriter<EnvironmentLayerMembershipChanged>,
) {
    if let Ok(member) = layers.get(discarded.entity) {
        changed.write(EnvironmentLayerMembershipChanged(member.root));
    }
}

fn project_environment_world_layers(
    mut commands: Commands,
    mut changed: MessageReader<EnvironmentLayerMembershipChanged>,
    visible_changes: Query<
        &WorldMember,
        (
            With<EnvironmentRenderLayer>,
            Or<(Changed<EnvironmentRenderLayer>, Changed<Visibility>)>,
        ),
    >,
    mut worlds: Query<(Option<&WorldMembers>, Option<&mut EnvironmentWorldLayers>)>,
    authored: Query<(&EnvironmentRenderLayer, &Visibility)>,
) {
    let dirty = changed
        .read()
        .map(|message| message.0)
        .chain(visible_changes.iter().map(|member| member.root))
        .collect::<EntityHashSet>();
    for world in dirty {
        let Ok((members, existing)) = worlds.get_mut(world) else {
            continue;
        };
        let mut next = EnvironmentWorldLayers::default();
        for (layer, visibility) in authored
            .iter_many(members.into_iter().flat_map(WorldMembers::iter))
            .matched()
        {
            if *visibility != Visibility::Hidden {
                *next.0.entry(layer.layer).or_insert(false) |= layer.clear_depth_after;
            }
        }
        if let Some(mut existing) = existing {
            existing.set_if_neq(next);
        } else {
            commands.entity(world).insert(next);
        }
    }
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
    layers: Query<Ref<EnvironmentWorldLayers>>,
    mut world_cameras: Query<
        (
            Entity,
            Ref<WorldMember>,
            &mut Camera,
            &mut Camera3d,
            Ref<Projection>,
            Ref<RenderTarget>,
            Ref<Exposure>,
            Ref<Msaa>,
            Option<Ref<Hdr>>,
            Option<Ref<DistanceFog>>,
            Option<&EnvironmentWorldCameraState>,
            Option<Ref<Children>>,
        ),
        (With<ZooCamera>, Without<EnvironmentStackCamera>),
    >,
    mut stacked_cameras: Query<(&mut EnvironmentStackCamera, &mut Camera3d), Without<ZooCamera>>,
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
        let authored = layers.get(member.root).ok();
        if !member.is_changed()
            && !children.as_ref().is_some_and(Ref::is_changed)
            && !authored.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let empty = BTreeMap::new();
        let authored_layers = authored.as_deref().map_or(&empty, |layers| &layers.0);
        let mut existing = BTreeMap::new();
        for child in children.as_deref().into_iter().flatten() {
            if let Ok((layer, ..)) = stacked_cameras.get(*child) {
                existing.insert(layer.layer, *child);
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
                    .map(|(&layer, &clear)| (Some(layer), clear)),
            )
            .enumerate()
        {
            let order = first_order + isize::try_from(ordinal).expect("u16 layer index");
            let color = if layer.is_some() {
                ClearColorConfig::None
            } else {
                background
            };
            let offset = order - view.order;
            if let Some(child) = existing.remove(&layer) {
                let Ok((mut binding, mut camera_3d)) = stacked_cameras.get_mut(child) else {
                    continue;
                };
                if binding.order_offset != offset {
                    binding.order_offset = offset;
                }
                set_depth_boundary(&mut camera_3d, clear_depth);
            } else {
                let mut camera_entity = commands.spawn((
                    EnvironmentStackCamera {
                        layer,
                        order_offset: offset,
                    },
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
                if hdr.is_some() {
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

#[allow(clippy::type_complexity)]
fn synchronize_environment_camera_optics(
    mut commands: Commands,
    mut removed_fog: RemovedComponents<DistanceFog>,
    mut removed_hdr: RemovedComponents<Hdr>,
    adjusted_stacked_cameras: Query<&ChildOf, Changed<EnvironmentStackCamera>>,
    world_cameras: Query<
        (
            Entity,
            Ref<Camera>,
            Ref<Camera3d>,
            Ref<Projection>,
            Ref<RenderTarget>,
            Ref<Exposure>,
            Ref<Msaa>,
            Option<Ref<Hdr>>,
            Option<Ref<DistanceFog>>,
            &Children,
        ),
        With<ZooCamera>,
    >,
    mut stacked: Query<
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
    let removed = removed_fog
        .read()
        .chain(removed_hdr.read())
        .collect::<EntityHashSet>();
    let changed_stacks = adjusted_stacked_cameras
        .iter()
        .map(ChildOf::parent)
        .collect::<EntityHashSet>();
    for (entity, view, view_3d, projection, target, exposure, msaa, hdr, fog, children) in
        &world_cameras
    {
        if !view.is_changed()
            && !view_3d.is_changed()
            && !projection.is_changed()
            && !target.is_changed()
            && !exposure.is_changed()
            && !msaa.is_changed()
            && !hdr.as_ref().is_some_and(Ref::is_changed)
            && !fog.as_ref().is_some_and(Ref::is_changed)
            && !removed.contains(&entity)
            && !changed_stacks.contains(&entity)
        {
            continue;
        }
        for child in children.iter() {
            let Ok((
                binding,
                mut camera,
                mut camera_3d,
                mut lens,
                mut output,
                mut light_exposure,
                mut samples,
                has_hdr,
                current_fog,
            )) = stacked.get_mut(child)
            else {
                continue;
            };
            let order = view.order + binding.order_offset;
            if camera.order != order {
                camera.order = order;
            }
            if view.is_changed() {
                let viewport_changed = match (&camera.viewport, &view.viewport) {
                    (Some(current), Some(source)) => {
                        current.physical_position != source.physical_position
                            || current.physical_size != source.physical_size
                            || current.depth != source.depth
                    }
                    (None, None) => false,
                    _ => true,
                };
                if viewport_changed {
                    camera.viewport = view.viewport.clone();
                }
                if camera.is_active != view.is_active {
                    camera.is_active = view.is_active;
                }
                if camera.sub_camera_view != view.sub_camera_view {
                    camera.sub_camera_view = view.sub_camera_view;
                }
            }
            if view_3d.is_changed()
                && camera_3d.depth_texture_usages.0 != view_3d.depth_texture_usages.0
            {
                camera_3d.depth_texture_usages = view_3d.depth_texture_usages;
            }
            if projection.is_changed() {
                *lens = (*projection).clone();
            }
            if target.is_changed() {
                *output = (*target).clone();
            }
            if light_exposure.ev100.to_bits() != exposure.ev100.to_bits() {
                *light_exposure = *exposure;
            }
            samples.set_if_neq(*msaa);
            if hdr.is_some() && !has_hdr {
                commands.entity(child).try_insert(Hdr);
            } else if hdr.is_none() && has_hdr && removed.contains(&entity) {
                commands.entity(child).try_remove::<Hdr>();
            }
            match (fog.as_ref(), current_fog) {
                (Some(fog), Some(mut current)) if fog.is_changed() => {
                    *current = (**fog).clone();
                }
                (Some(fog), None) => {
                    commands.entity(child).try_insert((**fog).clone());
                }
                (None, Some(_)) if removed.contains(&entity) => {
                    commands.entity(child).try_remove::<DistanceFog>();
                }
                _ => {}
            }
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
