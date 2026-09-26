use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::environment::{EnvironmentDefinition, EnvironmentVisualKind},
    AssetId,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::animation_playback::animation_joint_target_types::AnimationJointTarget;
use crate::plugins::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use crate::plugins::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy;
use crate::plugins::world_spawn::prefab_model_tint::PrefabModelTint;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabModel;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabPresentation;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabRenderable;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabShadowPolicy;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::environment_presentation_types::{
    EnvironmentCameraRelativeVisual, EnvironmentModelVisual, EnvironmentSunPosition,
    EnvironmentTexture, EnvironmentVisualOwner, EnvironmentVisualSampleIndex,
};

pub(super) fn hydrate_environment_visual_sample_renderer_entities(
    commands: &mut Commands,
    parent: Entity,
    member: &WorldMember,
    catalogue: WorldDefinitionsView<'_>,
    definition: &EnvironmentDefinition,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut sun_rigs = Vec::<(AssetId, Entity)>::new();
    for (sample_index, sample) in definition.visual_samples.iter().enumerate() {
        let id = sample.asset;
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
                Transform::from_scale(Vec3::splat(sample.scale)),
                Visibility::Inherited,
                *member,
            ));
            if matches!(&sample.kind, EnvironmentVisualKind::Sky) {
                child_commands.insert(EnvironmentCameraRelativeVisual::sky_layer(sample.scale));
            }
            if !sample.flags.contains_all(
                openzt2_game_data::world_definitions::environment::EnvironmentVisualFlags::USE_WORLD_LIGHTS,
            ) {
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
            Transform::from_scale(Vec3::splat(sample.scale)),
            Visibility::Inherited,
            *member,
        ));
        if matches!(&sample.kind, EnvironmentVisualKind::Sky) {
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

pub(in crate::plugins::environment) fn attach_environment_sun_visuals_to_loaded_animation_joints(
    mut commands: Commands,
    visuals: Query<(Entity, &EnvironmentSunPosition)>,
    joints: Query<(Entity, &AnimationJointTarget)>,
) {
    for (visual, position) in &visuals {
        let Some(joint) = joints.iter().find_map(|(entity, target)| {
            (target.animation_playback_controller_entity == position.rig
                && AssetId::from_key(target.joint_asset_key.as_str()) == position.node)
                .then_some(entity)
        }) else {
            continue;
        };
        commands.entity(joint).add_child(visual);
        commands.entity(visual).remove::<EnvironmentSunPosition>();
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
