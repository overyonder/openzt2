use bevy::camera::visibility::RenderLayers;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::world_definitions::environment::EnvironmentVisualFlags;
use openzt2_game_data::{
    world_definitions::environment::{EnvironmentDefinition, EnvironmentVisualKind},
    AssetId,
};
use openzt2_world_state::model_render::environment_render_layer::EnvironmentRenderLayer;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use bevy::gltf::{Gltf, GltfNode};
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use openzt2_world_state::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;

use super::environment_presentation_types::{
    EnvironmentCameraRelativeVisual, EnvironmentModelVisual, EnvironmentSunPosition,
    EnvironmentTexture, EnvironmentVisualOwner, EnvironmentVisualSampleIndex, PendingSunRigVisuals,
};

pub(super) fn hydrate_environment_visual_sample_renderer_entities(
    commands: &mut Commands,
    parent: Entity,
    member: &WorldMember,
    catalogue: WorldDefinitionsView<'_>,
    definition: &EnvironmentDefinition,
    terrain: &TerrainGrid,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut sun_rigs = Vec::<(AssetId, Entity)>::new();
    for (sample_index, sample) in definition.visual_samples.iter().enumerate() {
        let id = sample.asset;
        let mut transform = Transform::from_scale(Vec3::splat(sample.scale));
        if matches!(&sample.kind, EnvironmentVisualKind::Skirt) {
            transform.translation.y = terrain.presentation.water_editing.height_snap_base_metres;
        } else if matches!(&sample.kind, EnvironmentVisualKind::Sky)
            && !sample
                .flags
                .contains_all(EnvironmentVisualFlags::MOVE_WITH_CAMERA)
            && sample
                .flags
                .contains_all(EnvironmentVisualFlags::POSITION_AT_CENTER)
        {
            transform.translation.x =
                (terrain.width - 1) as f32 * terrain.units.cell_size_metres * 0.5;
            transform.translation.z =
                (terrain.height - 1) as f32 * terrain.units.cell_size_metres * 0.5;
        }
        if let Some(source) = catalogue.scene(id) {
            let color = Color::srgb(
                f32::from(sample.color_unorm[0]) / f32::from(u16::MAX),
                f32::from(sample.color_unorm[1]) / f32::from(u16::MAX),
                f32::from(sample.color_unorm[2]) / f32::from(u16::MAX),
            );
            let mut child_commands = commands.spawn((
                EnvironmentModelVisual,
                PrefabModelTint(color),
                EnvironmentVisualSampleIndex(sample_index as u32),
                EnvironmentVisualOwner(parent),
                PrefabPresentation::new(source),
                PrefabShadowPolicy {
                    casts: false,
                    receives: false,
                    visible_in_reflections: true,
                },
                transform,
                Visibility::Inherited,
                *member,
            ));
            if sample
                .flags
                .contains_all(EnvironmentVisualFlags::INTERPOLATE_VERTEX_COLORS)
                && !definition.visual_samples[..sample_index]
                    .iter()
                    .any(|earlier| {
                        earlier.layer == sample.layer
                            && earlier.order == sample.order
                            && earlier.kind == sample.kind
                    })
            {
                child_commands.insert(super::sky_vertex_color_interpolation::SkyVertexColorGroup);
            }
            if matches!(&sample.kind, EnvironmentVisualKind::Skirt) {
                child_commands.insert(RenderLayers::layer(TERRAIN_RENDER_LAYER));
            } else {
                child_commands.insert(EnvironmentRenderLayer {
                    layer: sample.layer,
                    clear_depth_after: sample
                        .flags
                        .contains_all(EnvironmentVisualFlags::CLEAR_DEPTH),
                });
            }
            if matches!(&sample.kind, EnvironmentVisualKind::Sky)
                && sample
                    .flags
                    .contains_all(EnvironmentVisualFlags::MOVE_WITH_CAMERA)
            {
                child_commands.insert(EnvironmentCameraRelativeVisual::sky_layer(sample.scale));
            }
            if !matches!(&sample.kind, EnvironmentVisualKind::Skirt)
                && !sample
                    .flags
                    .contains_all(EnvironmentVisualFlags::USE_WORLD_LIGHTS)
            {
                child_commands.insert(PrefabFixedFunctionWorldLightingPolicy {
                    enabled: false,
                    preserve_authored_material_lighting: false,
                });
            }
            if matches!(&sample.kind, EnvironmentVisualKind::Sun) {
                child_commands.insert(
                    PrefabAuthoredBillboardOrientationMode::from_authored_mode_and_local_rotation(
                        0,
                        Quat::IDENTITY,
                    ),
                );
            }
            let child = child_commands.id();
            commands.entity(parent).add_child(child);
            hydrate_environment_sun_position_rig(
                commands,
                parent,
                child,
                member,
                catalogue,
                sample,
                &mut sun_rigs,
            );
            continue;
        }
        let Some(image) = catalogue.texture_image(id) else {
            continue;
        };
        let color = Color::srgb(
            f32::from(sample.color_unorm[0]) / f32::from(u16::MAX),
            f32::from(sample.color_unorm[1]) / f32::from(u16::MAX),
            f32::from(sample.color_unorm[2]) / f32::from(u16::MAX),
        );
        let mut child_commands = commands.spawn((
            create_environment_texture_renderer_components(image, color, meshes, materials),
            EnvironmentVisualSampleIndex(sample_index as u32),
            EnvironmentVisualOwner(parent),
            transform,
            Visibility::Inherited,
            *member,
        ));
        if !matches!(&sample.kind, EnvironmentVisualKind::Skirt) {
            child_commands.insert(EnvironmentRenderLayer {
                layer: sample.layer,
                clear_depth_after: sample
                    .flags
                    .contains_all(EnvironmentVisualFlags::CLEAR_DEPTH),
            });
        }
        if matches!(&sample.kind, EnvironmentVisualKind::Sky)
            && sample
                .flags
                .contains_all(EnvironmentVisualFlags::MOVE_WITH_CAMERA)
        {
            child_commands.insert(EnvironmentCameraRelativeVisual::sky_layer(sample.scale));
        }
        if matches!(&sample.kind, EnvironmentVisualKind::Sun) {
            child_commands.insert(
                PrefabAuthoredBillboardOrientationMode::from_authored_mode_and_local_rotation(
                    0,
                    Quat::IDENTITY,
                ),
            );
        }
        let child = child_commands.id();
        commands.entity(parent).add_child(child);
        hydrate_environment_sun_position_rig(
            commands,
            parent,
            child,
            member,
            catalogue,
            sample,
            &mut sun_rigs,
        );
    }
}

fn hydrate_environment_sun_position_rig(
    commands: &mut Commands,
    parent: Entity,
    visual: Entity,
    member: &WorldMember,
    catalogue: WorldDefinitionsView<'_>,
    sample: &openzt2_game_data::world_definitions::environment::EnvironmentVisualSample,
    rigs: &mut Vec<(AssetId, Entity)>,
) {
    let position_model = sample.position_model;
    let node = sample.node;
    if position_model == AssetId::default() || node == AssetId::default() {
        return;
    }
    let rig = rigs
        .iter()
        .find_map(|(candidate, entity)| (*candidate == position_model).then_some(*entity))
        .or_else(|| {
            let handle = catalogue.model(position_model)?;
            let rig = commands
                .spawn((
                    PrefabModel {
                        model: position_model,
                        model_path: Box::default(),
                        handle: Some(handle),
                        scene_name: None,
                        material_name: None,
                    },
                    PrefabRenderable,
                    PrefabMaterialOverrides(Box::new([])),
                    PrefabShadowPolicy {
                        casts: false,
                        receives: false,
                        visible_in_reflections: true,
                    },
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    EnvironmentCameraRelativeVisual::sun_position_rig(),
                    *member,
                ))
                .id();
            commands.entity(parent).add_child(rig);
            rigs.push((position_model, rig));
            Some(rig)
        });
    if let Some(rig) = rig {
        commands
            .entity(visual)
            .insert(EnvironmentSunPosition { rig, node });
    }
}

pub(super) fn attach_sun_visual_when_position_model_is_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    names: Query<&Name>,
    rigs: Query<&PendingSunRigVisuals>,
    pending: Query<&EnvironmentSunPosition>,
) {
    let Some(visuals) = std::iter::once(ready.entity)
        .chain(parents.iter_ancestors::<ChildOf>(ready.entity))
        .find_map(|ancestor| rigs.get(ancestor).ok())
    else {
        return;
    };
    let mut unresolved: Vec<_> = visuals
        .iter()
        .filter_map(|visual| Some((visual, pending.get(visual).ok()?.node)))
        .collect();
    for node in children.iter_descendants_depth_first::<Children>(ready.entity) {
        let Ok(name) = names.get(node) else {
            continue;
        };
        let key = AssetId::from_key(&name.as_str().to_ascii_lowercase());
        unresolved.retain(|&(visual, requested)| {
            if requested != key {
                return true;
            }
            commands
                .entity(visual)
                .try_insert(ChildOf(node))
                .try_remove::<EnvironmentSunPosition>();
            false
        });
        if unresolved.is_empty() {
            break;
        }
    }
}

pub(super) fn attach_sun_visual_to_single_node_position_model(
    mesh: On<Add, Mesh3d>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    scene_roots: Query<(), With<WorldAssetRoot>>,
    rigs: Query<(&PrefabModel, &PendingSunRigVisuals)>,
    pending: Query<&EnvironmentSunPosition>,
    models: Option<Res<Assets<Gltf>>>,
    nodes: Option<Res<Assets<GltfNode>>>,
) {
    let Ok(gate) = parents.get(mesh.entity) else {
        return;
    };
    if scene_roots.contains(gate.parent()) {
        return;
    }
    let Some((model, visuals)) = parents
        .get(gate.parent())
        .ok()
        .and_then(|rig| rigs.get(rig.parent()).ok())
    else {
        return;
    };
    let Some(gltf) = models
        .as_ref()
        .and_then(|models| models.get(model.handle.as_ref()?))
        .filter(|gltf| {
            gltf.nodes.len() == 1 && gltf.animations.is_empty() && gltf.skins.is_empty()
        })
    else {
        return;
    };
    let Some(node) = nodes.as_ref().and_then(|nodes| nodes.get(&gltf.nodes[0])) else {
        return;
    };
    let requested = AssetId::from_key(&node.name.to_ascii_lowercase());
    for visual in visuals.iter() {
        if pending.get(visual).is_ok_and(|sun| sun.node == requested) {
            commands
                .entity(visual)
                .try_insert(ChildOf(mesh.entity))
                .try_remove::<EnvironmentSunPosition>();
        }
    }
}

pub(super) fn create_environment_texture_renderer_components(
    image: Handle<Image>,
    color: Color,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> (EnvironmentTexture, Mesh3d, MeshMaterial3d<StandardMaterial>) {
    let material = materials.add(StandardMaterial {
        base_color: color,
        base_color_texture: Some(image.clone()),
        emissive: color.to_linear(),
        emissive_texture: Some(image.clone()),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    (
        EnvironmentTexture {
            image,
            material: Some(material.clone()),
        },
        Mesh3d(meshes.add(Rectangle::from_size(Vec2::splat(2.0)))),
        MeshMaterial3d(material),
    )
}

pub(super) fn convert_authored_sky_keyframe_tint_to_bevy_color(
    keyframe: &openzt2_game_data::world_definitions::environment::EnvironmentSkyKeyframe,
) -> Color {
    Color::srgba(
        f32::from(keyframe.tint_unorm[0]) / f32::from(u16::MAX),
        f32::from(keyframe.tint_unorm[1]) / f32::from(u16::MAX),
        f32::from(keyframe.tint_unorm[2]) / f32::from(u16::MAX),
        f32::from(keyframe.tint_unorm[3]) / f32::from(u16::MAX),
    )
}
