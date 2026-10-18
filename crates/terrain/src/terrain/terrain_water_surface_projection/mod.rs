use crate::terrain::terrain_change_tracking_types::TerrainDirty;
use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_water_material_binding::bind_authored_water_material_values;
use crate::terrain::terrain_water_material_binding::bind_authored_water_surface_textures;
use crate::terrain::terrain_water_mesh_construction::construct_terrain_water_low_detail_mesh_groups;
use crate::terrain::terrain_water_mesh_construction::construct_terrain_water_surface_and_waterfall_mesh_groups;
use crate::terrain::terrain_waterfall_projection::spawn_authored_waterfall_groups;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::graphics_settings_types::FourLevelGraphicsDetail;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTransparentRenderStage;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRevision;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterSurface;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterfallSurface;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionSource,
};
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterLowDetailTextureState;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterScumScroll;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfacePassKind;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfaceRegion;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct WaterRegionMaterialProjectionQueries<'w, 's> {
    projection_status: Local<'s, bevy::platform::collections::HashMap<Entity, &'static str>>,
    scrolls: Query<'w, 's, &'static TerrainWaterScumScroll>,
    low_textures: Query<'w, 's, &'static TerrainWaterLowDetailTextureState>,
    changed: Query<
        'w,
        's,
        (),
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
        )>,
    >,
}

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
    memberships: Query<Ref<TerrainWaterMembershipIndex>>,
    region_sources: TerrainWaterRegionQuery,
    mut region_state: WaterRegionMaterialProjectionQueries,
    transforms: TransformHelper,
) {
    let Some(renderer_targets) = renderer_targets else {
        return;
    };
    for (chunk_entity, chunk, edited_samples, dirty, children, projected_revision) in
        &terrain_chunks
    {
        let mut report = |status| {
            if region_state.projection_status.insert(chunk_entity, status) != Some(status) {
                info!(target: "openzt2_water_projection", ?chunk_entity, ?status,
                    "water surface projection status changed");
            }
        };
        let membership = memberships
            .iter()
            .find(|index| index.terrain() == chunk.asset.id());
        let Some(membership) = membership else {
            report("membership pending");
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if projected_water.contains(child) {
                    commands.entity(child).despawn();
                }
            }
            commands
                .entity(chunk_entity)
                .remove::<TerrainWaterRevision>();
            continue;
        };
        let target_revision = dirty.map_or(0, |dirty| dirty.revision);
        if region_state.changed.is_empty()
            && !membership.is_changed()
            && !graphics_settings.is_changed()
            && projected_revision.is_some_and(|revision| revision.0 >= target_revision)
        {
            continue;
        }
        if region_state.changed.is_empty()
            && !membership.is_changed()
            && !graphics_settings.is_changed()
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
            report("terrain pending");
            continue;
        };
        let Some(water_material) =
            authored_materials.get(terrain_asset.water_surface_effect_material())
        else {
            report("surface material pending");
            continue;
        };
        let Some(water_pass_templates) = water_material
            .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
            .iter()
            .map(|handle| effect_pass_materials.get(handle).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            report("surface passes pending");
            continue;
        };
        let scum_required = terrain_asset
            .canonical_terrain_grid()
            .biomes
            .iter()
            .any(|biome| {
                biome
                    .water_presentation
                    .as_ref()
                    .is_some_and(|presentation| {
                        presentation
                            .surface_material
                            .configuration
                            .scum
                            .as_ref()
                            .and_then(|scum| scum.image.as_deref())
                            .and_then(|image| terrain_asset.texture_image(image))
                            .is_some()
                    })
            });
        let scum_pass_templates = if scum_required {
            let Some(templates) = terrain_asset
                .water_material("Materials/waterscum.bfmat")
                .and_then(|handle| authored_materials.get(handle))
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
                report("scum passes pending");
                continue;
            };
            Some(templates)
        } else {
            None
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
                    report("waterfall passes pending");
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
            report("waterfall scene pending");
            continue;
        }
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if projected_water.contains(child) {
                commands.entity(child).despawn();
            }
        }
        let sources = region_sources
            .iter()
            .filter(|(_, source)| source.terrain().id() == chunk.asset.id())
            .collect::<Vec<_>>();
        let Some(mesh_groups) = construct_terrain_water_surface_and_waterfall_mesh_groups(
            chunk,
            terrain_asset,
            edited_samples,
            &membership,
            &sources,
        ) else {
            report("surface geometry unresolved");
            commands
                .entity(chunk_entity)
                .remove::<TerrainWaterRevision>();
            continue;
        };
        report(if mesh_groups.is_empty() {
            "no surface geometry"
        } else {
            "surface geometry ready"
        });
        let medium_detail = graphics_settings.water_detail == FourLevelGraphicsDetail::Medium;
        let low_detail =
            graphics_settings.water_detail == FourLevelGraphicsDetail::Low || medium_detail;
        if low_detail {
            let Some(low_groups) = construct_terrain_water_low_detail_mesh_groups(
                chunk,
                terrain_asset,
                edited_samples,
                &membership,
                &sources,
            ) else {
                commands
                    .entity(chunk_entity)
                    .remove::<TerrainWaterRevision>();
                continue;
            };
            let Ok(chunk_transform) = transforms.compute_global_transform(chunk_entity) else {
                continue;
            };
            let source_to_bevy = Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W);
            let local_from_source = chunk_transform.to_matrix().inverse() * source_to_bevy;
            let mut ready = true;
            for low in low_groups {
                let Some(source) = region_sources.get(low.region_owner) else {
                    ready = false;
                    continue;
                };
                let Some(biome) = source.biome(terrain_asset) else {
                    ready = false;
                    continue;
                };
                let material_handle = if medium_detail {
                    terrain_asset.water_medium_detail_material(biome)
                } else {
                    terrain_asset.water_low_detail_material(biome)
                };
                let Some(material) =
                    material_handle.and_then(|handle| authored_materials.get(handle))
                else {
                    ready = false;
                    continue;
                };
                let Some(templates) = material
                    .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                    .iter()
                    .map(|handle| effect_pass_materials.get(handle).cloned())
                    .collect::<Option<Vec<_>>>()
                else {
                    ready = false;
                    continue;
                };
                let Some(height) = source.surface_height(terrain_asset) else {
                    ready = false;
                    continue;
                };
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                    low.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    ready = false;
                    continue;
                };
                let (minimum, maximum) = positions.iter().fold(
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                    |(minimum, maximum), position| {
                        let point = Vec2::new(position[0], -position[1]);
                        (minimum.min(point), maximum.max(point))
                    },
                );
                if !region_state.low_textures.contains(low.region_owner) {
                    commands
                        .entity(low.region_owner)
                        .insert(TerrainWaterLowDetailTextureState::default());
                }
                let group = commands
                    .spawn((
                        TerrainWaterSurface,
                        TerrainWaterSurfaceRegion(low.region_owner),
                        Transform::from_matrix(local_from_source),
                        Visibility::Inherited,
                        ChildOf(chunk_entity),
                    ))
                    .id();
                let mesh = meshes.add(low.mesh);
                let mut invocation = None;
                for (index, mut pass) in templates.into_iter().enumerate() {
                    let kind = if pass.has_fixed_function_reflection_projection() {
                        TerrainWaterSurfacePassKind::MediumEnvironment
                    } else if medium_detail {
                        TerrainWaterSurfacePassKind::MediumDetail
                    } else {
                        TerrainWaterSurfacePassKind::LowDetail
                    };
                    pass.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                    let entity = commands
                        .spawn((
                            TerrainWaterSurface,
                            TerrainWaterWaveStateKey(low.region_owner),
                            kind,
                            AuthoredTerrainWaterSurfaceEffectPass {
                                surface_plane_height: height,
                                horizontal_minimum: minimum,
                                horizontal_maximum: maximum,
                                maximum_absolute_input_height: 0.0,
                            },
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(effect_pass_materials.add(pass)),
                            NoFrustumCulling,
                            Transform::IDENTITY,
                            Visibility::Inherited,
                            RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                            ChildOf(group),
                        ))
                        .id();
                    let root = *invocation.get_or_insert(entity);
                    commands.entity(entity).insert(
                        AuthoredEffectTechniquePassSubmissionOrder::new(root, index)
                            .expect("water pass index fits u16")
                            .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
                    );
                }
            }
            if !ready {
                commands
                    .entity(chunk_entity)
                    .remove::<TerrainWaterRevision>();
                continue;
            }
        }
        for mesh_group in mesh_groups {
            if low_detail {
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
                continue;
            }
            let Some(presentation) = region_sources
                .get(mesh_group.region_owner)
                .and_then(|source| source.biome(terrain_asset))
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                continue;
            };
            let mesh = meshes.add(mesh_group.surface_mesh);
            let wave_key = TerrainWaterWaveStateKey(mesh_group.region_owner);
            let scum = presentation
                .surface_material
                .configuration
                .scum
                .as_ref()
                .and_then(|scum| scum.image.as_deref())
                .and_then(|image| terrain_asset.texture_image(image))
                .is_some();
            let surface_pass_templates = if scum {
                scum_pass_templates
                    .as_deref()
                    .expect("loaded scum material templates")
            } else {
                &water_pass_templates
            };
            let surface_group = commands
                .spawn((
                    TerrainWaterSurface,
                    TerrainWaterSurfaceRegion(mesh_group.region_owner),
                    wave_key,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(chunk_entity),
                ))
                .id();
            if scum {
                if !region_state.scrolls.contains(mesh_group.region_owner) {
                    commands
                        .entity(mesh_group.region_owner)
                        .insert(TerrainWaterScumScroll::default());
                }
            } else {
                commands
                    .entity(mesh_group.region_owner)
                    .remove::<TerrainWaterScumScroll>();
            }
            let mut water_technique_invocation = None;
            let passes: Vec<_> = if scum {
                surface_pass_templates
                    .iter()
                    .enumerate()
                    .map(|(index, pass)| (index, pass, TerrainWaterSurfacePassKind::Scum))
                    .collect()
            } else {
                vec![
                    (
                        0,
                        &surface_pass_templates[1],
                        TerrainWaterSurfacePassKind::FallbackReflection,
                    ),
                    (
                        1,
                        &surface_pass_templates[0],
                        TerrainWaterSurfacePassKind::Refraction,
                    ),
                    (
                        2,
                        &surface_pass_templates[1],
                        TerrainWaterSurfacePassKind::Reflection,
                    ),
                ]
            };
            for (pass_index, pass_template, pass_kind) in passes {
                let mut pass = pass_template.clone();
                if !bind_authored_water_surface_textures(
                    &mut pass,
                    &renderer_targets,
                    terrain_asset,
                    presentation,
                ) {
                    error!("water surface effect is missing its authored texture semantics");
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
                        pass_kind,
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
                        ChildOf(surface_group),
                    ))
                    .id();
                let technique_invocation = *water_technique_invocation.get_or_insert(entity);
                commands.entity(entity).insert(
                    AuthoredEffectTechniquePassSubmissionOrder::new(
                        technique_invocation,
                        pass_index,
                    )
                    .expect("the authored water surface effect pass index fits u16")
                    .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
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
