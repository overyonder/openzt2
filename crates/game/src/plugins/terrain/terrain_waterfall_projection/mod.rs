use super::terrain_water_mesh_construction::ConstructedTerrainWaterfallMeshGroup;
use super::terrain_water_presentation_types::TerrainWaterfallSurface;
use super::terrain_water_renderer_types::AuthoredTerrainWaterfallDecalEffectPass;
use super::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::audio::audio_playback_message_types::PlayAudioClip;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use crate::plugins::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_authored_waterfall_groups(
    commands: &mut Commands,
    audio_clip_requests: &mut MessageWriter<PlayAudioClip>,
    terrain_asset: &TerrainAsset,
    scene_prefabs: &Assets<ScenePrefabAsset>,
    effect_pass_materials: &mut Assets<EffectPassMaterial>,
    effect_uniform_buffers: &mut Assets<ShaderBuffer>,
    meshes: &mut Assets<Mesh>,
    waterfall_decal_pass_templates: Option<&[EffectPassMaterial]>,
    elapsed_seconds: f32,
    chunk_entity: Entity,
    waterfall_mesh_groups: Vec<ConstructedTerrainWaterfallMeshGroup>,
) {
    let Some(waterfall) = terrain_asset.canonical_terrain_grid().waterfall.as_ref() else {
        return;
    };
    let scene_prefab = terrain_asset
        .waterfall_scene_prefab()
        .and_then(|handle| scene_prefabs.get(handle));
    for waterfall_mesh_group in waterfall_mesh_groups {
        let mut waterfall_decal_effect_passes = waterfall_decal_pass_templates
            .map(<[EffectPassMaterial]>::to_vec)
            .unwrap_or_default();
        for pass in &mut waterfall_decal_effect_passes {
            let waterfall_decal_textures = terrain_asset
                .texture_image(&waterfall.decal_mask)
                .cloned()
                .zip(
                    terrain_asset
                        .texture_image(&waterfall.decal_detail)
                        .cloned(),
                );
            if waterfall_decal_textures.is_none_or(|(base_texture, detail_texture)| {
                !pass.configure_for_authored_terrain_decal(
                    base_texture,
                    detail_texture,
                    waterfall.alpha_blend,
                    waterfall.double_sided,
                    waterfall.rotate_detail,
                    waterfall.detail_v_scroll,
                    elapsed_seconds,
                )
            }) {
                error!("terrain waterfall could not bind its authored terraindecal.fx pass");
                waterfall_decal_effect_passes.clear();
                break;
            }
        }
        let waterfall_entity = commands
            .spawn((
                TerrainWaterfallSurface,
                Transform::from_translation(waterfall_mesh_group.local_anchor_translation),
                Visibility::Inherited,
                ChildOf(chunk_entity),
            ))
            .id();
        let waterfall_mesh = Mesh3d(meshes.add(waterfall_mesh_group.mesh));
        for (pass_index, mut waterfall_decal_effect_pass) in
            waterfall_decal_effect_passes.into_iter().enumerate()
        {
            waterfall_decal_effect_pass
                .recreate_uniform_buffers_for_runtime_material(effect_uniform_buffers);
            let material = effect_pass_materials.add(waterfall_decal_effect_pass);
            commands.spawn((
                AuthoredTerrainWaterfallDecalEffectPass {
                    material: material.clone(),
                    rotate_detail: waterfall.rotate_detail,
                    detail_vertical_scroll_per_second: waterfall.detail_v_scroll,
                },
                waterfall_mesh.clone(),
                MeshMaterial3d(material),
                RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(waterfall_entity),
                AuthoredEffectTechniquePassSubmissionOrder::new(waterfall_entity, pass_index)
                    .expect("the authored waterfall effect pass index fits u16"),
            ));
        }
        let particle_entity = commands
            .spawn((
                Transform::from_scale(Vec3::new(1.0, waterfall.particle_scrunch, 1.0)),
                Visibility::Inherited,
                ChildOf(waterfall_entity),
            ))
            .id();
        if let Some(scene_prefab) = scene_prefab {
            spawn_prefab_render_tree(commands, scene_prefab, particle_entity, false);
        }
        if let Some(sound) = terrain_asset.waterfall_audio() {
            audio_clip_requests.write(PlayAudioClip {
                clip: sound.clone(),
                emitter: Some(waterfall_entity),
                selection: waterfall_entity.to_bits(),
                force_looped: true,
            });
        }
    }
}

pub(super) fn advance_authored_terrain_waterfall_decal_detail_texture_coordinates(
    time: Res<Time>,
    projected_waterfall_decals: Query<&AuthoredTerrainWaterfallDecalEffectPass>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let elapsed_seconds = time.elapsed_secs();
    for projected_waterfall_decal in &projected_waterfall_decals {
        let Some(effect_pass_material) =
            effect_pass_materials.get_mut_untracked(&projected_waterfall_decal.material)
        else {
            continue;
        };
        if !effect_pass_material.advance_authored_terrain_decal_detail_texture_coordinates(
            projected_waterfall_decal.rotate_detail,
            projected_waterfall_decal.detail_vertical_scroll_per_second,
            elapsed_seconds,
        ) {
            continue;
        }
        effect_pass_material
            .write_fixed_function_transforms_to_persistent_buffer(&mut effect_uniform_buffers);
    }
}
