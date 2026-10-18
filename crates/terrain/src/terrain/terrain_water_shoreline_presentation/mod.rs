use crate::terrain::terrain_water_shoreline_geometry::projection::PreparedTerrainShorelineMeshGroups;
use crate::terrain::terrain_water_shoreline_geometry::ConstructedTerrainShorelineMeshGroup;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsEffectQuality;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainWaterShoreConfiguration;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTransparentRenderStage;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_shoreline_presentation_types::TerrainWaterShorelinePresentation;
use openzt2_zoo_state::terrain::terrain_water_shoreline_presentation_types::TerrainWaterShorelineSurface;
use openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn submit_prepared_terrain_shoreline_mesh_groups(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    graphics: Res<GraphicsSettings>,
    materials: Res<Assets<MaterialAsset>>,
    mut effect_passes: ResMut<Assets<EffectPassMaterial>>,
    mut uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut prepared: Query<(
        Entity,
        &TerrainWaterShorelinePresentation,
        &NativeTerrainWaterGeometricWaveShaderState,
        &TerrainWaterRegionLight,
        &mut PreparedTerrainShorelineMeshGroups,
    )>,
) {
    for (owner, presentation, waves, light, mut prepared) in &mut prepared {
        let Some(source) = regions.get(owner) else {
            continue;
        };
        if prepared.groups.is_empty() {
            continue;
        }
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let shore = configuration.surface_material.configuration.shore.as_ref();
        let mut retry = Vec::new();
        for (parent, group) in std::mem::take(&mut prepared.groups) {
            if let Err(group) = spawn_authored_terrain_shoreline_mesh_group(
                &mut commands,
                parent,
                owner,
                group,
                terrain,
                graphics.effects,
                &materials,
                &mut effect_passes,
                &mut uniform_buffers,
                &mut meshes,
                presentation,
                waves,
                Some(light.raw_base_rgba().truncate()),
                shore,
            ) {
                retry.push((parent, group));
            }
        }
        prepared.groups = retry;
    }
}

pub(super) fn initialize_terrain_shoreline_material_animation(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    initialized: Query<(), With<TerrainWaterShorelinePresentation>>,
) {
    for (entity, source) in regions.iter() {
        if initialized.contains(entity) {
            continue;
        }
        let resolved = memberships.iter().any(|membership| {
            membership.terrain() == source.terrain().id()
                && membership.accepted_points(entity).next().is_some()
        });
        if !resolved {
            continue;
        }
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        commands
            .entity(entity)
            .insert(TerrainWaterShorelinePresentation::from_configuration(
                configuration.surface_material.configuration.shore.as_ref(),
            ));
    }
}

/// Returns the unsubmitted geometry while any material dependency is unavailable.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_authored_terrain_shoreline_mesh_group(
    commands: &mut Commands,
    parent: Entity,
    material_owner: Entity,
    group: ConstructedTerrainShorelineMeshGroup,
    terrain: &TerrainAsset,
    quality: GraphicsEffectQuality,
    materials: &Assets<MaterialAsset>,
    effect_passes: &mut Assets<EffectPassMaterial>,
    uniform_buffers: &mut Assets<ShaderBuffer>,
    meshes: &mut Assets<Mesh>,
    presentation: &TerrainWaterShorelinePresentation,
    wave_state: &NativeTerrainWaterGeometricWaveShaderState,
    active_environment_colour: Option<Vec3>,
    shore: Option<&TerrainWaterShoreConfiguration>,
) -> Result<Entity, ConstructedTerrainShorelineMeshGroup> {
    let Some(material) = terrain
        .water_material("Materials/watershore.bfmat")
        .and_then(|handle| materials.get(handle))
    else {
        return Err(group);
    };
    let templates = material.evaluated_pass_material_assets_for_effect_quality(quality);
    let [template] = templates else {
        return Err(group);
    };
    let Some(mut pass) = effect_passes.get(template).cloned() else {
        return Err(group);
    };
    if !bind_shoreline_material_textures(&mut pass, terrain, shore) {
        return Err(group);
    }
    bind_shoreline_material_values(&mut pass, shore, presentation, active_environment_colour);
    wave_state.bind_wave_channels_to_effect_pass(group.surface_plane_height, &mut pass);
    pass.recreate_uniform_buffers_for_runtime_material(uniform_buffers);
    let entity = commands
        .spawn((
            TerrainWaterShorelineSurface {
                material_owner,
                biome_index: group.biome_index,
                surface_plane_height: group.surface_plane_height,
            },
            Mesh3d(meshes.add(group.mesh)),
            MeshMaterial3d(effect_passes.add(pass)),
            NoFrustumCulling,
            Transform::IDENTITY,
            Visibility::Inherited,
            RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
            ChildOf(parent),
        ))
        .id();
    commands.entity(entity).insert(
        AuthoredEffectTechniquePassSubmissionOrder::new(entity, 0)
            .expect("the shoreline's sole authored pass index fits u16")
            .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
    );
    Ok(entity)
}

pub(super) fn advance_terrain_shoreline_material_animation(
    time: Res<Time>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut materials: Query<(Entity, &mut TerrainWaterShorelinePresentation)>,
) {
    for (owner, mut presentation) in &mut materials {
        let Some(source) = regions.get(owner) else {
            continue;
        };
        if let Some(configuration) = terrain_assets
            .get(source.terrain())
            .and_then(|terrain| source.biome(terrain))
            .and_then(|biome| biome.water_presentation.as_ref())
        {
            presentation
                .apply_configuration(configuration.surface_material.configuration.shore.as_ref());
        }
        presentation.advance(time.delta_secs());
    }
}

pub(super) fn update_terrain_shoreline_material_animation_bindings(
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    materials: Query<(
        &TerrainWaterShorelinePresentation,
        &NativeTerrainWaterGeometricWaveShaderState,
        &TerrainWaterRegionLight,
    )>,
    mut surfaces: Query<(
        &TerrainWaterShorelineSurface,
        &MeshMaterial3d<EffectPassMaterial>,
        &mut Visibility,
    )>,
    mut effect_passes: ResMut<Assets<EffectPassMaterial>>,
    mut uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (surface, handle, mut visibility) in &mut surfaces {
        let Ok((presentation, waves, light)) = materials.get(surface.material_owner) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(source) = regions.get(surface.material_owner) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(height) = source.surface_height(terrain) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(mut pass) = effect_passes.get_mut(&handle.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        bind_shoreline_material_values(
            &mut pass,
            configuration.surface_material.configuration.shore.as_ref(),
            presentation,
            Some(light.raw_base_rgba().truncate()),
        );
        waves.bind_wave_channels_to_effect_pass(height, &mut pass);
        pass.write_programmable_float_registers_to_persistent_buffers(&mut uniform_buffers);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

pub(super) fn despawn_terrain_shoreline_surfaces_without_material_owners(
    mut commands: Commands,
    owners: TerrainWaterRegionQuery,
    surfaces: Query<(Entity, &TerrainWaterShorelineSurface)>,
) {
    for (entity, surface) in &surfaces {
        if owners.get(surface.material_owner).is_none() {
            commands.entity(entity).despawn();
        }
    }
}

pub(super) fn bind_shoreline_material_textures(
    material: &mut EffectPassMaterial,
    terrain: &TerrainAsset,
    configuration: Option<&TerrainWaterShoreConfiguration>,
) -> bool {
    let mut all_bound = true;
    for (semantic, source, fallback) in [
        (
            "RippleTop",
            configuration.and_then(|configuration| configuration.ripple_top.as_deref()),
            "Materials/waterlayout",
        ),
        (
            "RippleBot",
            configuration.and_then(|configuration| configuration.ripple_bottom.as_deref()),
            "Materials/waterlayout",
        ),
        (
            "RippleMask",
            configuration.and_then(|configuration| configuration.ripple_mask.as_deref()),
            "Materials/watermask",
        ),
    ] {
        let Some(texture) = source
            .and_then(|source| terrain.texture_image(source))
            .or_else(|| terrain.texture_image(fallback))
        else {
            all_bound = false;
            continue;
        };
        all_bound &= material.replace_texture_asset_for_effect_semantic(semantic, texture.clone());
    }
    all_bound
}

pub(super) fn bind_shoreline_material_values(
    material: &mut EffectPassMaterial,
    configuration: Option<&TerrainWaterShoreConfiguration>,
    presentation: &TerrainWaterShorelinePresentation,
    active_environment_colour: Option<Vec3>,
) {
    let mut tint = configuration
        .and_then(|configuration| configuration.tint_rgba255)
        .map_or(Vec4::new(1.0, 1.0, 1.0, 0.5), |colour| {
            Vec4::from_array(colour.map(|channel| f32::from(channel) / 255.0))
        });
    if let Some(colour) = active_environment_colour {
        tint.x *= colour.x;
        tint.y *= colour.y;
        tint.z *= colour.z;
    }
    material.bind_float_vector_effect_semantic("Tint", tint);
    material.bind_float_vector_effect_semantic("BotUVMap", presentation.bottom_uv_map());
    material.bind_float_vector_effect_semantic("TopUVMap", presentation.top_uv_map());
}
