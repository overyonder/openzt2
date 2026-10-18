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
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_BUMP_COMBINATION_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_REFLECTION_TARGET_SIZE;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_REFRACTION_TARGET_SIZE;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::WATER_RENDER_TARGET_SIDE;

#[derive(Component)]
struct AuthoredTerrainWaterBumpCombinationQuad;

pub(super) fn initialize_authored_terrain_water_renderer_targets(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    existing: Option<Res<AuthoredTerrainWaterRendererTargets>>,
) {
    if existing.is_some() {
        return;
    }
    let Some((_, terrain_asset)) = terrain_assets.iter().next() else {
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
    let bump_mesh = Mesh3d(meshes.add(Rectangle::new(2.0, 2.0)));
    let mut bump_technique_invocation = None;
    for (pass_index, material) in water_bump_combination_materials.iter().enumerate() {
        let entity = commands
            .spawn((
                AuthoredTerrainWaterBumpCombinationQuad,
                bump_mesh.clone(),
                MeshMaterial3d(material.clone()),
                RenderLayers::layer(WATER_BUMP_COMBINATION_RENDER_LAYER),
            ))
            .id();
        let technique_invocation = *bump_technique_invocation.get_or_insert(entity);
        commands.entity(entity).insert(
            AuthoredEffectTechniquePassSubmissionOrder::new(technique_invocation, pass_index)
                .expect("the authored water bump effect pass index fits u16"),
        );
    }
    let bump_camera = commands
        .spawn((
            Camera3d::default(),
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
        ))
        .id();
    let refraction_camera = spawn_water_scene_render_target_camera(
        &mut commands,
        refraction.clone(),
        -2,
        WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        false,
    );
    let reflection_camera = spawn_water_scene_render_target_camera(
        &mut commands,
        reflection.clone(),
        -1,
        WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        true,
    );
    let fallback_reflection_camera = spawn_water_scene_render_target_camera(
        &mut commands,
        fallback_reflection.clone(),
        -4,
        WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        true,
    );
    commands.insert_resource(AuthoredTerrainWaterRendererTargets {
        combined_bump_map,
        reflection,
        refraction,
        fallback_reflection,
        bump_camera,
        reflection_camera,
        refraction_camera,
        fallback_reflection_camera,
        water_bump_combination_materials,
        elapsed_seconds: 0.0,
        active_water_plane_height: None,
        reflection_world_extent: Vec2::splat(50.0),
        refraction_world_extent: Vec2::splat(50.0),
        fallback_reflection_world_extent: Vec2::splat(50.0),
    });
}

fn spawn_water_scene_render_target_camera(
    commands: &mut Commands,
    target: Handle<Image>,
    order: isize,
    effect_pass_render_layer: usize,
    invert_culling: bool,
) -> Entity {
    commands
        .spawn((
            Camera3d::default(),
            Camera {
                order,
                is_active: false,
                invert_culling,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Image(target.into()),
            RenderLayers::from_layers(&[0, TERRAIN_RENDER_LAYER, effect_pass_render_layer]),
            EffectPassMaterialRenderView {
                render_layer: effect_pass_render_layer,
            },
        ))
        .id()
}
