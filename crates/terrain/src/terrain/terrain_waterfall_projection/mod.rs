use crate::terrain::{
    terrain_change_tracking_types::TerrainDirty,
    terrain_water_mesh_construction::{
        construct_terrain_waterfall_mesh_groups, ConstructedTerrainWaterfallMeshGroup,
    },
    water_regions::authored_membership::TerrainWaterMembershipChanged,
};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::{
    graphics_settings_types::GraphicsSettings, material::material_asset_types::MaterialAsset,
    scene_prefab::ScenePrefabAsset,
};
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterfallSurface;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterfallDecalEffectPass;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_zoo_state::terrain::{
    terrain_chunk_presentation_types::TerrainRenderChunk,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery},
    terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets,
};

#[derive(Component)]
pub(super) struct WaterfallAnchor(Vec2);

#[derive(Component)]
#[relationship(relationship_target = RegionWaterfalls)]
pub(super) struct WaterfallRegion(Entity);

#[derive(Component)]
#[relationship_target(relationship = WaterfallRegion, linked_spawn)]
pub(super) struct RegionWaterfalls(Vec<Entity>);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct WaterfallDraws<'w, 's> {
    groups: Query<
        'w,
        's,
        (
            &'static WaterfallAnchor,
            &'static WaterfallRegion,
            &'static mut Transform,
            Option<&'static Children>,
        ),
        (
            With<TerrainWaterfallSurface>,
            Without<
                openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk,
            >,
        ),
    >,
    passes: Query<
        'w,
        's,
        (
            &'static mut AuthoredTerrainWaterfallDecalEffectPass,
            &'static mut Mesh3d,
            &'static mut MeshMaterial3d<EffectPassMaterial>,
        ),
    >,
}

#[derive(Component)]
pub(super) struct TerrainWaterfallGeometry(Vec<(Entity, ConstructedTerrainWaterfallMeshGroup)>);

#[derive(Component)]
pub(super) struct TerrainWaterfallDrawsPending;

#[allow(clippy::type_complexity)]
pub(super) fn prepare_changed_terrain_waterfall_geometry(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut terrain_events: MessageReader<AssetEvent<TerrainAsset>>,
    mut changes: MessageReader<TerrainWaterMembershipChanged>,
    membership: Query<&TerrainWaterMembershipIndex>,
    sources: TerrainWaterRegionQuery,
    chunks: Query<
        (
            Entity,
            &TerrainChunk,
            Option<&EditedTerrainSamples>,
            Option<Ref<TerrainDirty>>,
        ),
        With<TerrainRenderChunk>,
    >,
    mut prepared: Query<&mut TerrainWaterfallGeometry>,
) {
    let changed: bevy::platform::collections::HashSet<_> = terrain_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect();
    let changes: Vec<_> = changes.read().collect();
    for (entity, chunk, edited, dirty) in &chunks {
        let id = chunk.asset.id();
        let (Some(asset), Some(index)) = (
            terrain_assets.get(id),
            membership.iter().find(|index| index.terrain() == id),
        ) else {
            if prepared.contains(entity) {
                commands.entity(entity).remove::<TerrainWaterfallGeometry>();
            }
            continue;
        };
        let cells = u32::from(chunk.side.saturating_sub(1)).max(1);
        let local = changes.iter().any(|change| {
            change.affects_chunk(chunk)
                || (change.terrain == id
                    && change.regions.as_ref().is_some_and(|owners| {
                        owners.iter().any(|owner| {
                            index
                                .render_tiles(*owner)
                                .any(|tile| (tile * 4 / cells).as_ivec2() == chunk.source_coord)
                        })
                    }))
        });
        if prepared.contains(entity)
            && !dirty.is_some_and(|dirty| dirty.is_changed())
            && !changed.contains(&id)
            && !local
        {
            continue;
        }
        let sources: Vec<_> = sources
            .iter()
            .filter(|(_, source)| source.terrain().id() == id)
            .collect();
        let Some(next) =
            construct_terrain_waterfall_mesh_groups(chunk, asset, edited, index, &sources)
        else {
            continue;
        };
        if let Ok(mut current) = prepared.get_mut(entity) {
            current.0 = next;
        } else {
            commands
                .entity(entity)
                .insert(TerrainWaterfallGeometry(next));
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn publish_terrain_waterfall_draws(
    mut commands: Commands,
    mut audio: MessageWriter<PlayAudioClip>,
    terrains: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    settings: Res<GraphicsSettings>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    renderer: Query<&AuthoredTerrainWaterRendererTargets>,
    chunks: Query<
        (
            Entity,
            &TerrainChunk,
            Option<Ref<TerrainWaterfallGeometry>>,
            Has<TerrainWaterfallDrawsPending>,
            Option<&Children>,
        ),
        With<TerrainRenderChunk>,
    >,
    mut draws: WaterfallDraws,
) {
    for (entity, chunk, geometry, pending, children) in &chunks {
        let Some(geometry) = geometry else {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if draws.groups.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
            continue;
        };
        if !geometry.is_changed() && !settings.is_changed() && !pending {
            continue;
        }
        if geometry.0.is_empty() {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if draws.groups.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
            if pending {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterfallDrawsPending>();
            }
            continue;
        }
        let Some(terrain) = terrains.get(&chunk.asset) else {
            continue;
        };
        if !pending {
            commands.entity(entity).insert(TerrainWaterfallDrawsPending);
        }
        let Ok(renderer) = renderer.single() else {
            continue;
        };
        if terrain
            .waterfall_scene_prefab()
            .is_some_and(|prefab| !prefabs.contains(prefab.id()))
        {
            continue;
        }
        let Some(templates) = authored_materials
            .get(terrain.waterfall_decal_effect_material())
            .and_then(|material| {
                material
                    .evaluated_pass_material_assets_for_effect_quality(settings.effects)
                    .iter()
                    .map(|handle| materials.get(handle).cloned())
                    .collect::<Option<Vec<_>>>()
            })
        else {
            continue;
        };
        reconcile_authored_waterfall_groups(
            &mut commands,
            &mut audio,
            terrain,
            &prefabs,
            &mut materials,
            &mut buffers,
            &mut meshes,
            Some(&templates),
            renderer.elapsed_seconds,
            entity,
            geometry.0.clone(),
            &mut draws,
            children,
        );
        commands
            .entity(entity)
            .remove::<TerrainWaterfallDrawsPending>();
    }
}

#[allow(clippy::too_many_arguments)]
fn reconcile_authored_waterfall_groups(
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
    waterfall_mesh_groups: Vec<(Entity, ConstructedTerrainWaterfallMeshGroup)>,
    draws: &mut WaterfallDraws,
    children: Option<&Children>,
) {
    let mut previous: Vec<_> = children
        .into_iter()
        .flat_map(|children| children.iter())
        .filter(|&child| draws.groups.contains(child))
        .collect();
    let Some(waterfall) = terrain_asset.canonical_terrain_grid().waterfall.as_ref() else {
        for entity in previous {
            commands.entity(entity).try_despawn();
        }
        return;
    };
    let scene_prefab = terrain_asset
        .waterfall_scene_prefab()
        .and_then(|handle| scene_prefabs.get(handle));
    for (owner, waterfall_mesh_group) in waterfall_mesh_groups {
        let anchor = waterfall_mesh_group.local_anchor_translation;
        let retained = previous
            .iter()
            .position(|&entity| {
                draws
                    .groups
                    .get(entity)
                    .is_ok_and(|(group, region, ..)| region.0 == owner && group.0 == anchor.xz())
            })
            .map(|position| previous.remove(position));
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
        let waterfall_entity = retained.unwrap_or_else(|| {
            commands
                .spawn((
                    TerrainWaterfallSurface,
                    WaterfallAnchor(anchor.xz()),
                    WaterfallRegion(owner),
                    Transform::from_translation(anchor),
                    Visibility::Inherited,
                    ChildOf(chunk_entity),
                ))
                .id()
        });
        let mut existing = Vec::new();
        if let Ok((_, _, mut pose, children)) = draws.groups.get_mut(waterfall_entity) {
            if pose.translation != anchor {
                pose.translation = anchor;
            }
            existing.extend(
                children
                    .into_iter()
                    .flat_map(|children| children.iter())
                    .filter(|&child| draws.passes.contains(child)),
            );
        }
        let waterfall_mesh = Mesh3d(meshes.add(waterfall_mesh_group.mesh));
        let retain_passes = existing.len() == waterfall_decal_effect_passes.len();
        if !retain_passes {
            for child in existing.drain(..) {
                commands.entity(child).try_despawn();
            }
        }
        for (pass_index, mut pass) in waterfall_decal_effect_passes.into_iter().enumerate() {
            pass.recreate_uniform_buffers_for_runtime_material(effect_uniform_buffers);
            let material = effect_pass_materials.add(pass);
            let state = AuthoredTerrainWaterfallDecalEffectPass {
                material: material.clone(),
                rotate_detail: waterfall.rotate_detail,
                detail_vertical_scroll_per_second: waterfall.detail_v_scroll,
            };
            if retain_passes {
                let (mut current, mut mesh, mut handle) = draws
                    .passes
                    .get_mut(existing[pass_index])
                    .expect("ordered waterfall draw");
                *current = state;
                *mesh = waterfall_mesh.clone();
                handle.0 = material;
            } else {
                commands.spawn((
                    state,
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
        }
        if retained.is_some() {
            continue;
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
    for entity in previous {
        commands.entity(entity).try_despawn();
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
