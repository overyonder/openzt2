use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_model_material_pass_projection::EffectPassMaterialRenderView;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::world_spawn::{
    selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
    world_membership_types::WorldRoot,
};
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfacePassKind;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_BUMP_COMBINATION_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_REFLECTION_TARGET_SIZE;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_REFRACTION_TARGET_SIZE;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_RENDER_TARGET_SIDE;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::{
    AuthoredTerrainWaterRendererTargets, TerrainWaterCaptureExtent, TerrainWaterCaptureRole,
};

#[derive(Component)]
struct AuthoredTerrainWaterBumpCombinationQuad;

#[derive(Component)]
#[relationship(relationship_target = WorldWaterCaptureRigs)]
pub(super) struct WaterCaptureRigOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = WaterCaptureRigOf, linked_spawn)]
pub(super) struct WorldWaterCaptureRigs(Vec<Entity>);

#[derive(Component)]
pub(super) struct WaterCaptureRefreshPending;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn initialize_authored_terrain_water_renderer_targets(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut terrain_events: MessageReader<AssetEvent<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    worlds: Query<
        (
            Entity,
            Ref<SelectedWorldTerrainAssetHandle>,
            Option<&WorldWaterCaptureRigs>,
            Has<WaterCaptureRefreshPending>,
        ),
        With<WorldRoot>,
    >,
) {
    let Ok((world, terrain, rigs, pending)) = worlds.single() else {
        terrain_events.clear();
        return;
    };
    let id = terrain.0.id();
    let reloaded = terrain_events.read().any(|event| event.is_modified(id));
    terrain_events.clear();
    if rigs.is_some() && !terrain.is_changed() && !reloaded && !pending {
        return;
    }
    if !pending {
        commands.entity(world).insert(WaterCaptureRefreshPending);
    }
    let Some(terrain_asset) = terrain_assets.get(&terrain.0) else {
        return;
    };
    let Some(water_bump_material_asset) =
        authored_materials.get(terrain_asset.water_bump_combination_effect_material())
    else {
        return;
    };
    let Some(water_bump_combination_templates) = water_bump_material_asset
        .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
        .iter()
        .map(|handle| effect_pass_materials.get(handle).cloned())
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let water_bump_combination_materials = water_bump_combination_templates
        .into_iter()
        .map(|mut material| {
            material.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
            effect_pass_materials.add(material)
        })
        .collect::<Box<[_]>>();
    let combined_bump_map = images.add(Image::new_target_texture(
        WATER_RENDER_TARGET_SIDE,
        WATER_RENDER_TARGET_SIDE,
        TextureFormat::Rgba8Unorm,
        None,
    ));
    let reflection = images.add(Image::new_target_texture(
        WATER_REFLECTION_TARGET_SIZE.x,
        WATER_REFLECTION_TARGET_SIZE.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let refraction = images.add(Image::new_target_texture(
        WATER_REFRACTION_TARGET_SIZE.x,
        WATER_REFRACTION_TARGET_SIZE.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let fallback_reflection = images.add(Image::new_target_texture(
        WATER_REFLECTION_TARGET_SIZE.x,
        WATER_REFLECTION_TARGET_SIZE.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    if let Some(rigs) = rigs {
        for rig in rigs.iter() {
            commands.entity(rig).try_despawn();
        }
    }
    commands
        .entity(world)
        .remove::<WaterCaptureRefreshPending>();
    let bump_mesh = Mesh3d(meshes.add(Rectangle::new(2.0, 2.0)));
    let rig = commands
        .spawn((
            WaterCaptureRigOf(world),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    let mut bump_technique_invocation = None;
    for (pass_index, material) in water_bump_combination_materials.iter().enumerate() {
        let entity = commands
            .spawn((
                AuthoredTerrainWaterBumpCombinationQuad,
                bump_mesh.clone(),
                MeshMaterial3d(material.clone()),
                RenderLayers::layer(WATER_BUMP_COMBINATION_RENDER_LAYER),
                ChildOf(rig),
            ))
            .id();
        let technique_invocation = *bump_technique_invocation.get_or_insert(entity);
        commands.entity(entity).insert(
            AuthoredEffectTechniquePassSubmissionOrder::new(technique_invocation, pass_index)
                .expect("the authored water bump effect pass index fits u16"),
        );
    }
    commands.spawn((
        Camera3d::default(),
        TerrainWaterCaptureRole::Bump,
        TerrainWaterCaptureExtent(Vec2::splat(50.0)),
        // This fullscreen effect uses no clustered lighting. Keep one cell
        // because Bevy 0.19's GPU path creates a zero-sized texture for None.
        bevy::light::cluster::ClusterConfig::Single,
        Camera {
            order: -3,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed {
                width: 2.0,
                height: 2.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 1.0),
        RenderTarget::Image(combined_bump_map.clone().into()),
        RenderLayers::layer(WATER_BUMP_COMBINATION_RENDER_LAYER),
        ChildOf(rig),
    ));
    spawn_water_scene_render_target_camera(
        &mut commands,
        rig,
        refraction.clone(),
        TerrainWaterCaptureRole::Refraction,
        -2,
        WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        false,
    );
    spawn_water_scene_render_target_camera(
        &mut commands,
        rig,
        reflection.clone(),
        TerrainWaterCaptureRole::Reflection,
        -1,
        WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        true,
    );
    spawn_water_scene_render_target_camera(
        &mut commands,
        rig,
        fallback_reflection.clone(),
        TerrainWaterCaptureRole::FallbackReflection,
        -4,
        WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        true,
    );
    commands
        .entity(rig)
        .insert(AuthoredTerrainWaterRendererTargets {
            combined_bump_map,
            reflection,
            refraction,
            fallback_reflection,
            water_bump_combination_materials,
            elapsed_seconds: 0.0,
        });
}

fn spawn_water_scene_render_target_camera(
    commands: &mut Commands,
    rig: Entity,
    target: Handle<Image>,
    role: TerrainWaterCaptureRole,
    order: isize,
    effect_pass_render_layer: usize,
    invert_culling: bool,
) {
    commands.spawn((
        Camera3d::default(),
        role,
        TerrainWaterCaptureExtent(Vec2::splat(50.0)),
        Camera {
            order,
            is_active: false,
            invert_culling,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderTarget::Image(target.into()),
        RenderLayers::from_layers(&[0, TERRAIN_RENDER_LAYER, effect_pass_render_layer]),
        ChildOf(rig),
    ));
}

/// A water camera needs model draws only while the zoo has a surface that
/// samples the scene. Keep them across camera moves and visibility changes.
pub(super) fn reconcile_water_material_render_views(
    mut commands: Commands,
    targets: Query<&Children, With<AuthoredTerrainWaterRendererTargets>>,
    captures: Query<(Entity, &TerrainWaterCaptureRole)>,
    passes: Query<&TerrainWaterSurfacePassKind>,
    views: Query<(), With<EffectPassMaterialRenderView>>,
) {
    let Ok(children) = targets.single() else {
        return;
    };
    let needs_capture = passes.iter().any(|pass| !pass.uses_stitch_geometry());
    for (camera, role) in captures.iter_many(children.iter()).flatten() {
        let render_layer = match role {
            TerrainWaterCaptureRole::Bump => continue,
            TerrainWaterCaptureRole::Reflection => WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
            TerrainWaterCaptureRole::Refraction => WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
            TerrainWaterCaptureRole::FallbackReflection => {
                WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER
            }
        };
        if needs_capture != views.contains(camera) {
            if needs_capture {
                commands
                    .entity(camera)
                    .insert(EffectPassMaterialRenderView { render_layer });
            } else {
                commands
                    .entity(camera)
                    .remove::<EffectPassMaterialRenderView>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_capture_views_follow_the_lifetime_of_water_that_samples_them() {
        let mut app = App::new();
        let rig = app
            .world_mut()
            .spawn(AuthoredTerrainWaterRendererTargets {
                combined_bump_map: default(),
                reflection: default(),
                refraction: default(),
                fallback_reflection: default(),
                water_bump_combination_materials: default(),
                elapsed_seconds: 0.0,
            })
            .id();
        let [bump_camera, reflection_camera, refraction_camera, fallback_reflection_camera] = [
            TerrainWaterCaptureRole::Bump,
            TerrainWaterCaptureRole::Reflection,
            TerrainWaterCaptureRole::Refraction,
            TerrainWaterCaptureRole::FallbackReflection,
        ]
        .map(|role| app.world_mut().spawn((role, ChildOf(rig))).id());
        app.add_systems(Update, reconcile_water_material_render_views);
        let pass = app
            .world_mut()
            .spawn(TerrainWaterSurfacePassKind::LowDetail)
            .id();
        for (kind, expected) in [
            (TerrainWaterSurfacePassKind::LowDetail, false),
            (TerrainWaterSurfacePassKind::Reflection, true),
            (TerrainWaterSurfacePassKind::MediumEnvironment, false),
        ] {
            app.world_mut().entity_mut(pass).insert(kind);
            app.update();
            for (camera, layer) in [
                (
                    reflection_camera,
                    WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
                ),
                (
                    refraction_camera,
                    WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
                ),
                (
                    fallback_reflection_camera,
                    WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
                ),
            ] {
                assert_eq!(
                    app.world()
                        .get::<EffectPassMaterialRenderView>(camera)
                        .map(|v| v.render_layer),
                    expected.then_some(layer)
                );
            }
            assert!(!app
                .world()
                .entity(bump_camera)
                .contains::<EffectPassMaterialRenderView>());
        }
    }
}
