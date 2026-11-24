use super::terrain_water_renderer_inputs::TerrainWaterLowDetailTextureState;
use crate::terrain::terrain_change_tracking_types::TerrainDirty;
use crate::terrain::terrain_water_material_binding::bind_authored_water_material_values;
use crate::terrain::terrain_water_material_binding::bind_authored_water_surface_textures;
use crate::terrain::terrain_water_mesh_construction::{
    construct_terrain_water_low_detail_mesh_groups, construct_terrain_water_surface_mesh_groups,
    ConstructedTerrainWaterLowDetailMeshGroup, ConstructedTerrainWaterMeshGroup,
};
use crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use openzt2_game_assets::graphics_settings_types::FourLevelGraphicsDetail;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTransparentRenderStage;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterSurface;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterfallSurface;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionSource,
};
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterScumScroll;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfacePassKind;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfaceRegion;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::{
    AuthoredTerrainWaterSurfaceEffectPass, AuthoredTerrainWaterfallDecalEffectPass,
    TerrainWaterSurfaceLocalFromWorld,
};

#[derive(Component)]
pub(in crate::terrain) struct TerrainWaterSurfaceProjectionPending;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct WaterRegionMaterialProjectionQueries<'w, 's> {
    scrolls: Query<'w, 's, &'static TerrainWaterScumScroll>,
    low_textures: Query<'w, 's, &'static TerrainWaterLowDetailTextureState>,
}

#[derive(Component)]
pub(super) enum TerrainWaterSurfaceGeometry {
    Stitch(Vec<ConstructedTerrainWaterLowDetailMeshGroup>),
    Geometric(Vec<ConstructedTerrainWaterMeshGroup>),
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn prepare_changed_terrain_water_surfaces(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    settings: Res<GraphicsSettings>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>), With<TerrainRenderChunk>>,
    dirty: Query<(), Or<(Added<TerrainRenderChunk>, Changed<TerrainDirty>)>>,
    changed_regions: Query<
        Entity,
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
        )>,
    >,
    mut membership_changes: MessageReader<TerrainWaterMembershipChanged>,
    mut geometries: Query<&mut TerrainWaterSurfaceGeometry>,
) {
    let changes: Vec<_> = membership_changes.read().collect();
    let changed_owners: Vec<_> = changed_regions.iter().collect();
    let low_detail = matches!(
        settings.water_detail,
        FourLevelGraphicsDetail::Low | FourLevelGraphicsDetail::Medium
    );
    let mut jobs = Vec::new();
    for (entity, chunk, edited) in &chunks {
        let id = chunk.asset.id();
        let (Some(asset), Some(membership)) = (
            assets.get(id),
            memberships.iter().find(|index| index.terrain() == id),
        ) else {
            if geometries.contains(entity) {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterSurfaceGeometry>();
            }
            continue;
        };
        let cells = u32::from(chunk.side.saturating_sub(1)).max(1);
        if geometries.contains(entity)
            && !settings.is_changed()
            && !dirty.contains(entity)
            && !changes.iter().any(|change| change.affects_chunk(chunk))
            && !changed_owners.iter().any(|owner| {
                membership
                    .render_tiles(*owner)
                    .any(|tile| (tile * 4 / cells).as_ivec2() == chunk.source_coord)
            })
        {
            continue;
        }
        jobs.push((entity, chunk, edited, asset, membership));
    }
    if jobs.is_empty() {
        return;
    }
    let selected: Vec<_> = regions.iter().collect();
    let prepared: Vec<_> = ComputeTaskPool::get_or_init(TaskPool::default).scope(|scope| {
        for (entity, chunk, edited, asset, membership) in jobs {
            let selected = &selected;
            scope.spawn(async move {
                let geometry = if low_detail {
                    TerrainWaterSurfaceGeometry::Stitch(
                        construct_terrain_water_low_detail_mesh_groups(
                            chunk, asset, edited, membership, selected,
                        )?,
                    )
                } else {
                    TerrainWaterSurfaceGeometry::Geometric(
                        construct_terrain_water_surface_mesh_groups(
                            chunk, asset, membership, selected,
                        )?,
                    )
                };
                Some((entity, geometry))
            });
        }
    });
    for (entity, next) in prepared.into_iter().flatten() {
        if let Ok(mut geometry) = geometries.get_mut(entity) {
            *geometry = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SurfaceDraws<'w, 's> {
    groups: Query<
        'w,
        's,
        (
            &'static TerrainWaterSurfaceRegion,
            Option<&'static Children>,
            &'static mut Transform,
        ),
        (
            Without<TerrainRenderChunk>,
            Without<TerrainWaterfallSurface>,
        ),
    >,
    passes: Query<
        'w,
        's,
        (
            &'static TerrainWaterSurfacePassKind,
            &'static TerrainWaterSurfacePassSlot,
            &'static mut AuthoredTerrainWaterSurfaceEffectPass,
            &'static mut Mesh3d,
            &'static mut MeshMaterial3d<EffectPassMaterial>,
            &'static mut TerrainWaterSurfaceLocalFromWorld,
        ),
        Without<AuthoredTerrainWaterfallDecalEffectPass>,
    >,
}

#[derive(Component)]
struct TerrainWaterSurfacePassSlot(usize);

#[allow(clippy::too_many_arguments)]
fn project_surface_draw_group(
    commands: &mut Commands,
    draws: &mut SurfaceDraws,
    previous: Option<Entity>,
    chunk: Entity,
    owner: Entity,
    chunk_pose: &Transform,
    pose: Transform,
    mesh: Handle<Mesh>,
    passes: Vec<(
        usize,
        TerrainWaterSurfacePassKind,
        AuthoredTerrainWaterSurfaceEffectPass,
        Handle<EffectPassMaterial>,
    )>,
) {
    let local_from_world = (chunk_pose.to_matrix() * pose.to_matrix()).inverse();
    let group = previous.unwrap_or_else(|| {
        commands
            .spawn((
                TerrainWaterSurface,
                TerrainWaterSurfaceRegion(owner),
                pose,
                Visibility::Inherited,
                ChildOf(chunk),
            ))
            .id()
    });
    let mut children = Vec::new();
    if let Ok((_, existing, mut transform)) = draws.groups.get_mut(group) {
        transform.set_if_neq(pose);
        children.extend(
            existing
                .into_iter()
                .flat_map(|children| children.iter())
                .filter(|&entity| draws.passes.contains(entity)),
        );
    }
    let retain = children.len() == passes.len()
        && children
            .iter()
            .zip(&passes)
            .all(|(&entity, (index, kind, ..))| {
                draws
                    .passes
                    .get(entity)
                    .is_ok_and(|(previous, slot, ..)| previous == kind && slot.0 == *index)
            });
    if retain {
        for (entity, (_, _, surface, material)) in children.into_iter().zip(passes) {
            let (_, _, mut current, mut mesh_handle, mut material_handle, mut coordinates) =
                draws.passes.get_mut(entity).expect("ordered surface pass");
            current.set_if_neq(surface);
            coordinates.set_if_neq(TerrainWaterSurfaceLocalFromWorld(local_from_world));
            if mesh_handle.0 != mesh {
                mesh_handle.0 = mesh.clone();
            }
            if material_handle.0 != material {
                material_handle.0 = material;
            }
        }
        return;
    }
    for child in children {
        commands.entity(child).try_despawn();
    }
    let mut first = None;
    for (index, kind, surface, material) in passes {
        let entity = commands
            .spawn((
                TerrainWaterSurface,
                TerrainWaterWaveStateKey(owner),
                kind,
                TerrainWaterSurfacePassSlot(index),
                surface,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                TerrainWaterSurfaceLocalFromWorld(local_from_world),
                Aabb::default(),
                NoFrustumCulling,
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                ChildOf(group),
            ))
            .id();
        let invocation = *first.get_or_insert(entity);
        commands.entity(entity).insert(
            AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                .expect("water pass index fits u16")
                .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn publish_terrain_water_surfaces(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    renderer_targets: Query<Ref<AuthoredTerrainWaterRendererTargets>>,
    terrain_chunks: Query<
        (
            Entity,
            &TerrainChunk,
            &Transform,
            Option<Ref<TerrainWaterSurfaceGeometry>>,
            Option<&Children>,
            Has<TerrainWaterSurfaceProjectionPending>,
        ),
        With<TerrainRenderChunk>,
    >,
    projected_water: Query<(), With<TerrainWaterSurface>>,
    region_sources: TerrainWaterRegionQuery,
    (region_state, mut surface_draws): (WaterRegionMaterialProjectionQueries, SurfaceDraws),
) {
    let rig_replaced = renderer_targets.iter().any(|rig| rig.is_added());
    for (chunk_entity, chunk, chunk_transform, geometry, children, pending) in &terrain_chunks {
        if !pending
            && !graphics_settings.is_changed()
            && !rig_replaced
            && geometry
                .as_ref()
                .is_some_and(|geometry| !geometry.is_changed())
        {
            continue;
        }
        if !pending {
            commands
                .entity(chunk_entity)
                .insert(TerrainWaterSurfaceProjectionPending);
        }
        let Some(geometry) = geometry else {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if projected_water.contains(child) {
                    commands.entity(child).despawn();
                }
            }
            continue;
        };
        let Ok(renderer_targets) = renderer_targets.single() else {
            continue;
        };
        let _rebuild_span = info_span!("rebuild_chunk_water_surfaces").entered();
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
                continue;
            };
            Some(templates)
        } else {
            None
        };
        let mut previous_groups: bevy::ecs::entity::EntityHashMap<Entity> = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|group| {
                let (owner, ..) = surface_draws.groups.get(group).ok()?;
                Some((owner.0, group))
            })
            .collect();
        let medium_detail = graphics_settings.water_detail == FourLevelGraphicsDetail::Medium;
        if let TerrainWaterSurfaceGeometry::Stitch(low_groups) = &*geometry {
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
                let mesh = meshes.add(low.mesh.clone());
                let mut passes = Vec::new();
                for (index, mut pass) in templates.into_iter().enumerate() {
                    pass.retain_authored_draw_order();
                    let kind = if pass.has_fixed_function_reflection_projection() {
                        TerrainWaterSurfacePassKind::MediumEnvironment
                    } else if medium_detail {
                        TerrainWaterSurfacePassKind::MediumDetail
                    } else {
                        TerrainWaterSurfacePassKind::LowDetail
                    };
                    pass.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                    passes.push((
                        index,
                        kind,
                        AuthoredTerrainWaterSurfaceEffectPass {
                            surface_plane_height: height,
                            horizontal_minimum: minimum,
                            horizontal_maximum: maximum,
                            maximum_absolute_input_height: 0.0,
                        },
                        effect_pass_materials.add(pass),
                    ));
                }
                project_surface_draw_group(
                    &mut commands,
                    &mut surface_draws,
                    previous_groups.remove(&low.region_owner),
                    chunk_entity,
                    low.region_owner,
                    chunk_transform,
                    Transform::from_matrix(local_from_source),
                    mesh,
                    passes,
                );
            }
            if !ready {
                continue;
            }
        }
        let mesh_groups = match &*geometry {
            TerrainWaterSurfaceGeometry::Geometric(groups) => groups.as_slice(),
            TerrainWaterSurfaceGeometry::Stitch(_) => &[],
        };
        for mesh_group in mesh_groups {
            let Some(presentation) = region_sources
                .get(mesh_group.region_owner)
                .and_then(|source| source.biome(terrain_asset))
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                continue;
            };
            let Some(mesh) = mesh_group.surface_mesh.clone() else {
                continue;
            };
            let mesh = meshes.add(mesh);
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
            let mut draw_passes = Vec::new();
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
                pass.retain_authored_draw_order();
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
                draw_passes.push((
                    pass_index,
                    pass_kind,
                    AuthoredTerrainWaterSurfaceEffectPass {
                        surface_plane_height: mesh_group.surface_plane_height,
                        horizontal_minimum: mesh_group.horizontal_minimum,
                        horizontal_maximum: mesh_group.horizontal_maximum,
                        maximum_absolute_input_height: mesh_group.maximum_absolute_input_height,
                    },
                    effect_pass_materials.add(pass),
                ));
            }
            project_surface_draw_group(
                &mut commands,
                &mut surface_draws,
                previous_groups.remove(&mesh_group.region_owner),
                chunk_entity,
                mesh_group.region_owner,
                chunk_transform,
                Transform::IDENTITY,
                mesh,
                draw_passes,
            );
        }
        for &group in previous_groups.values() {
            commands.entity(group).try_despawn();
        }
        commands
            .entity(chunk_entity)
            .remove::<TerrainWaterSurfaceProjectionPending>();
    }
}
