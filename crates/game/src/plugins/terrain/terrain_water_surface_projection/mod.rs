use super::terrain_change_tracking_types::TerrainDirty;
use super::terrain_change_tracking_types::TerrainDirtyFlags;
use super::terrain_chunk_presentation_types::TerrainRenderChunk;
use super::terrain_chunk_types::EditedTerrainSamples;
use super::terrain_chunk_types::TerrainChunk;
use super::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use super::terrain_water_material_binding::bind_authored_water_material_values;
use super::terrain_water_material_binding::bind_authored_water_surface_textures;
use super::terrain_water_mesh_construction::construct_terrain_water_surface_and_waterfall_mesh_groups;
use super::terrain_water_presentation_types::TerrainWaterRevision;
use super::terrain_water_presentation_types::TerrainWaterSurface;
use super::terrain_water_presentation_types::TerrainWaterfallSurface;
use super::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use super::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use super::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use super::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use super::terrain_waterfall_projection::spawn_authored_waterfall_groups;
use crate::assets::material::material_asset_types::MaterialAsset;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::audio::audio_playback_message_types::PlayAudioClip;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_changed_terrain_water_surfaces_and_waterfalls(
    mut commands: Commands,
    mut audio_clip_requests: MessageWriter<PlayAudioClip>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    scene_prefabs: Res<Assets<ScenePrefabAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    renderer_targets: Option<ResMut<AuthoredTerrainWaterRendererTargets>>,
    terrain_chunks: Query<
        (
            Entity,
            &TerrainChunk,
            Option<&EditedTerrainSamples>,
            Option<&TerrainDirty>,
            Option<&Children>,
            Option<&TerrainWaterRevision>,
        ),
        With<TerrainRenderChunk>,
    >,
    projected_water: Query<(), Or<(With<TerrainWaterSurface>, With<TerrainWaterfallSurface>)>>,
) {
    let Some(mut renderer_targets) = renderer_targets else {
        return;
    };
    renderer_targets
        .wave_states
        .retain(|key, _| terrain_assets.contains(key.0));
    for (chunk_entity, chunk, edited_samples, dirty, children, projected_revision) in
        &terrain_chunks
    {
        let target_revision = dirty.map_or(0, |dirty| dirty.revision);
        if !graphics_settings.is_changed()
            && projected_revision.is_some_and(|revision| revision.0 >= target_revision)
        {
            continue;
        }
        if !graphics_settings.is_changed()
            && projected_revision.is_some()
            && dirty.is_some_and(|dirty| {
                !dirty.flags.contains(TerrainDirtyFlags::HEIGHT)
                    && !dirty.flags.contains(TerrainDirtyFlags::WATER)
            })
        {
            commands
                .entity(chunk_entity)
                .insert(TerrainWaterRevision(target_revision));
            continue;
        }
        let Some(terrain_asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let Some(water_material) =
            authored_materials.get(terrain_asset.water_surface_effect_material())
        else {
            continue;
        };
        let Some(water_pass_templates) = water_material
            .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
            .iter()
            .map(|handle| effect_pass_materials.get(handle).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let waterfall_decal_pass_templates =
            if terrain_asset.canonical_terrain_grid().waterfall.is_some() {
                let Some(templates) = authored_materials
                    .get(terrain_asset.waterfall_decal_effect_material())
                    .and_then(|material| {
                        material
                            .evaluated_pass_material_assets_for_effect_quality(
                                graphics_settings.effects,
                            )
                            .iter()
                            .map(|handle| effect_pass_materials.get(handle).cloned())
                            .collect::<Option<Vec<_>>>()
                    })
                else {
                    continue;
                };
                Some(templates)
            } else {
                None
            };
        if water_pass_templates.len() != 2 {
            error!(
                passes = water_pass_templates.len(),
                "waterflat.fx did not produce its authored refraction and reflection passes"
            );
            continue;
        }
        if terrain_asset
            .waterfall_scene_prefab()
            .is_some_and(|handle| scene_prefabs.get(handle).is_none())
        {
            continue;
        }
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if projected_water.contains(child) {
                commands.entity(child).despawn();
            }
        }
        let mesh_groups = construct_terrain_water_surface_and_waterfall_mesh_groups(
            chunk,
            terrain_asset,
            edited_samples,
        );
        for mesh_group in mesh_groups {
            let Some(presentation) = terrain_asset
                .canonical_terrain_grid()
                .biomes
                .get(usize::from(mesh_group.biome_index))
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                continue;
            };
            let mesh = meshes.add(mesh_group.surface_mesh);
            let wave_key = TerrainWaterWaveStateKey(chunk.asset.id(), mesh_group.biome_index);
            renderer_targets
                .wave_states
                .entry(wave_key)
                .or_insert_with(|| {
                    NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
                        presentation.surface_material.geometric_waves,
                        presentation.surface_material.ripple_waves,
                        u32::from(mesh_group.biome_index),
                    )
                });
            let mut water_technique_invocation = None;
            for (pass_index, pass_template) in water_pass_templates.iter().enumerate() {
                let mut pass = pass_template.clone();
                if !bind_authored_water_surface_textures(
                    &mut pass,
                    &renderer_targets,
                    terrain_asset,
                    presentation,
                ) {
                    error!("waterflat.fx is missing one of its authored texture semantics");
                    continue;
                }
                bind_authored_water_material_values(
                    &mut pass,
                    presentation,
                    mesh_group.surface_plane_height,
                );
                pass.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                let entity = commands
                    .spawn((
                        TerrainWaterSurface,
                        wave_key,
                        AuthoredTerrainWaterSurfaceEffectPass {
                            surface_plane_height: mesh_group.surface_plane_height,
                            horizontal_minimum: mesh_group.horizontal_minimum,
                            horizontal_maximum: mesh_group.horizontal_maximum,
                            maximum_absolute_input_height: mesh_group.maximum_absolute_input_height,
                        },
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(effect_pass_materials.add(pass)),
                        NoFrustumCulling,
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                        ChildOf(chunk_entity),
                    ))
                    .id();
                let technique_invocation = *water_technique_invocation.get_or_insert(entity);
                commands.entity(entity).insert(
                    AuthoredEffectTechniquePassSubmissionOrder::new(
                        technique_invocation,
                        pass_index,
                    )
                    .expect("the authored water surface effect pass index fits u16"),
                );
            }
            spawn_authored_waterfall_groups(
                &mut commands,
                &mut audio_clip_requests,
                terrain_asset,
                &scene_prefabs,
                &mut effect_pass_materials,
                &mut effect_uniform_buffers,
                &mut meshes,
                waterfall_decal_pass_templates.as_deref(),
                renderer_targets.elapsed_seconds,
                chunk_entity,
                mesh_group.waterfall_mesh_groups,
            );
        }
        commands
            .entity(chunk_entity)
            .insert(TerrainWaterRevision(target_revision));
    }
}
