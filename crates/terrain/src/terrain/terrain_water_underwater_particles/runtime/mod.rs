use crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    effect::{
        effect_document_runtime_queries::collect_root_effect_emitter_ids,
        explicit_particle_emission::{AppendParticleEffectPoints, ParticleEffectPointEmitter},
        ParticleEffectDocumentAsset, SpawnParticleEffectEmitter,
    },
    graphics_settings_types::{FourLevelGraphicsDetail, GraphicsSettings},
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_game_data::particle::{
    authored_particle_system::AuthoredParticleSystemModifier, ParticleEffectDocument,
};
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::terrain::{
    terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
        TerrainWaterRegionRetired, TerrainWaterRegionSource,
    },
    terrain_water_underwater_particle_types::{
        TerrainWaterBubbleEmitter, TerrainWaterParticleEffects, TerrainWaterSuspensionEmitter,
        TerrainWaterSuspensionVolume,
    },
};

use super::{
    append_initial_suspension_emission, append_suspension_density_top_up,
    bubble_spawn_delay_seconds, bubble_spawn_point, select_bubble_detail,
    suspension_emitter_density,
};

fn water_quality(settings: &GraphicsSettings) -> u32 {
    match settings.water_detail {
        FourLevelGraphicsDetail::Low => 0,
        FourLevelGraphicsDetail::Medium => 1,
        FourLevelGraphicsDetail::High => 2,
        FourLevelGraphicsDetail::Ultra => 3,
    }
}

fn configured_particle_effects<'a>(
    terrain: &'a TerrainAsset,
    water: &openzt2_game_data::terrain::TerrainBiomeWaterPresentation,
    quality: u32,
) -> (
    Option<&'a Handle<ParticleEffectDocumentAsset>>,
    Option<&'a Handle<ParticleEffectDocumentAsset>>,
) {
    let configuration = &water.surface_material.configuration;
    let suspension = configuration
        .suspension
        .as_ref()
        .and_then(|config| config.system.as_deref())
        .filter(|system| !system.is_empty())
        .and_then(|system| terrain.water_particle_effect(system));
    let bubbles = (quality > 0)
        .then(|| {
            select_bubble_detail(&configuration.bubbles, quality)
                .unwrap_or(&configuration.bubbles_fallback)
        })
        .and_then(|detail| detail.system.as_deref())
        .filter(|system| !system.is_empty())
        .and_then(|system| terrain.water_particle_effect(system));
    (suspension, bubbles)
}

fn changed_asset<A: Asset>(event: &AssetEvent<A>) -> Option<AssetId<A>> {
    match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Removed { id } => Some(id),
        AssetEvent::Unused { .. } => None,
    }
}

#[derive(SystemParam)]
pub(in crate::terrain) struct UnderwaterParticleEffectChanges<'w, 's> {
    membership: MessageReader<'w, 's, TerrainWaterMembershipChanged>,
    terrain: MessageReader<'w, 's, AssetEvent<TerrainAsset>>,
    effects: MessageReader<'w, 's, AssetEvent<ParticleEffectDocumentAsset>>,
    changed_regions: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
            Added<TerrainWaterRegionRetired>,
        )>,
    >,
    removed_sources: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    removed_edits: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    reactivated: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
    removed_indexes: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn synchronize_region_underwater_particle_effects(
    mut commands: Commands,
    phase: Res<State<GamePhase>>,
    settings: Res<GraphicsSettings>,
    terrains: Res<Assets<TerrainAsset>>,
    effects: Res<Assets<ParticleEffectDocumentAsset>>,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    regions: TerrainWaterRegionQuery,
    roots: Query<(Entity, &ChildOf, &TerrainWaterParticleEffects)>,
    region_children: Query<&Children>,
    transforms: TransformHelper,
    mut random: ResMut<NativeVisualRandomStream>,
    mut changes: UnderwaterParticleEffectChanges,
) {
    let membership: Vec<_> = changes.membership.read().collect();
    let terrains_changed: bevy::platform::collections::HashSet<_> =
        changes.terrain.read().filter_map(changed_asset).collect();
    let effects_changed: bevy::platform::collections::HashSet<_> =
        changes.effects.read().filter_map(changed_asset).collect();
    let changed_owners: bevy::ecs::entity::EntityHashSet = changes
        .changed_regions
        .iter()
        .chain(changes.removed_sources.read())
        .chain(changes.removed_edits.read())
        .chain(changes.reactivated.read())
        .collect();
    let all =
        phase.is_changed() | settings.is_changed() | (changes.removed_indexes.read().count() > 0);
    if !all
        && membership.is_empty()
        && terrains_changed.is_empty()
        && effects_changed.is_empty()
        && changed_owners.is_empty()
    {
        return;
    }
    let quality = water_quality(&settings);
    for (entity, parent, state) in &roots {
        let owner = parent.parent();
        if !all
            && !changed_owners.contains(&owner)
            && !regions.get(owner).is_some_and(|source| {
                terrains_changed.contains(&source.terrain().id())
                    || membership
                        .iter()
                        .any(|change| change.affects_region(source.terrain().id(), owner))
            })
        {
            continue;
        }
        let valid = *phase.get() == GamePhase::InGame
            && state.quality == quality
            && regions.get(parent.parent()).is_some_and(|source| {
                indexes
                    .iter()
                    .any(|index| index.terrain() == source.terrain().id())
                    && terrains.get(source.terrain()).is_some_and(|terrain| {
                        source
                            .biome(terrain)
                            .and_then(|biome| biome.water_presentation.as_ref())
                            .is_some_and(|water| {
                                let (suspension, bubbles) =
                                    configured_particle_effects(terrain, water, quality);
                                state.suspension_effect.as_ref() == suspension
                                    && state.bubble_effect.as_ref() == bubbles
                            })
                    })
            });
        if !valid {
            commands.entity(entity).despawn();
        }
    }
    if *phase.get() != GamePhase::InGame {
        return;
    }
    for (owner, source) in regions.iter() {
        if !indexes
            .iter()
            .any(|index| index.terrain() == source.terrain().id())
        {
            continue;
        }
        let Some(terrain) = terrains.get(source.terrain()) else {
            continue;
        };
        let Some(water) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let (suspension, bubble_effect) = configured_particle_effects(terrain, water, quality);
        if !all
            && !changed_owners.contains(&owner)
            && !terrains_changed.contains(&source.terrain().id())
            && !membership
                .iter()
                .any(|change| change.affects_region(source.terrain().id(), owner))
            && !suspension
                .into_iter()
                .chain(bubble_effect)
                .any(|handle| effects_changed.contains(&handle.id()))
        {
            continue;
        }
        if region_children.get(owner).is_ok_and(|children| {
            roots
                .iter_many(children.iter())
                .flatten()
                .any(|(_, _, state)| {
                    state.quality == quality
                        && state.suspension_effect.as_ref() == suspension
                        && state.bubble_effect.as_ref() == bubble_effect
                })
        }) {
            continue;
        }
        if suspension
            .into_iter()
            .chain(bubble_effect)
            .any(|handle| effects.get(handle).is_none())
        {
            continue;
        }
        let Ok(transform) = transforms.compute_global_transform(owner) else {
            continue;
        };
        let root = commands
            .spawn((
                TerrainWaterParticleEffects {
                    quality,
                    suspension_effect: suspension.cloned(),
                    bubble_effect: bubble_effect.cloned(),
                },
                Transform::from_matrix(transform.to_matrix().inverse()),
                Visibility::Inherited,
                ChildOf(owner),
            ))
            .id();
        if let Some(handle) = suspension {
            let Some(effect) = effects.get(handle) else {
                continue;
            };
            if let ParticleEffectDocument::Effect(document) = &effect.particle_document {
                let root_emitters = collect_root_effect_emitter_ids(&effect.particle_document);
                for (emission_index, emitter) in document
                    .emitters
                    .iter()
                    .filter(|emitter| root_emitters.contains(&emitter.emitter_id))
                    .enumerate()
                {
                    let particle_radius = emitter
                        .modifiers
                        .iter()
                        .find_map(|modifier| match modifier {
                            AuthoredParticleSystemModifier::InitialSizeRange { max, .. } => {
                                Some(*max)
                            }
                            _ => None,
                        })
                        .unwrap_or(0.0);
                    commands.spawn((
                        TerrainWaterSuspensionEmitter {
                            emission_index,
                            initial_fill: true,
                            particle_radius,
                        },
                        ParticleEffectPointEmitter::new(handle.clone(), emitter.emitter_id),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(root),
                    ));
                }
            }
        }
        if let Some(handle) = bubble_effect {
            let Some(effect) = effects.get(handle) else {
                continue;
            };
            for (emission_index, emitter) in
                collect_root_effect_emitter_ids(&effect.particle_document)
                    .into_iter()
                    .enumerate()
            {
                commands.spawn((
                    TerrainWaterBubbleEmitter {
                        emitter,
                        emission_index,
                        remaining_seconds: (8.0 - 7.0 * random.unit_f64()) as f32,
                    },
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    ChildOf(root),
                ));
            }
        }
    }
}

pub(in crate::terrain) fn update_underwater_particle_visibility(
    view: Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    terrains: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    roots: Query<&ChildOf, With<TerrainWaterParticleEffects>>,
    mut emitters: Query<
        (
            &ChildOf,
            &mut Visibility,
            Has<TerrainWaterSuspensionEmitter>,
        ),
        Or<(
            With<TerrainWaterSuspensionEmitter>,
            With<TerrainWaterBubbleEmitter>,
        )>,
    >,
) {
    for (parent, mut visibility, suspension) in &mut emitters {
        let enabled = (|| {
            let owner = roots.get(parent.parent()).ok()?.parent();
            let source = regions.get(owner)?;
            let terrain = terrains.get(source.terrain())?;
            let below = view.camera_position()?.y <= source.surface_height(terrain)?;
            Some(below && (!suspension || view.camera_region() == Some(owner)))
        })()
        .unwrap_or(false);
        *visibility = if enabled {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[derive(Component, PartialEq)]
pub(in crate::terrain) struct TerrainWaterSuspensionVolumes {
    volumes: Box<[TerrainWaterSuspensionVolume]>,
    total_m3: f32,
}

#[allow(clippy::type_complexity)]
pub(in crate::terrain) fn project_region_suspension_volumes(
    mut commands: Commands,
    terrains: Res<Assets<TerrainAsset>>,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    regions: TerrainWaterRegionQuery,
    changed_regions: Query<
        (),
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
        )>,
    >,
    mut projected: Query<&mut TerrainWaterSuspensionVolumes>,
    mut membership: MessageReader<TerrainWaterMembershipChanged>,
    mut terrain_events: MessageReader<AssetEvent<TerrainAsset>>,
) {
    let membership: Vec<_> = membership.read().collect();
    let terrain_events: Vec<_> = terrain_events.read().collect();
    for (owner, source) in regions.iter() {
        let Some((terrain, index)) = terrains.get(source.terrain()).zip(
            indexes
                .iter()
                .find(|index| index.terrain() == source.terrain().id()),
        ) else {
            if projected.contains(owner) {
                commands
                    .entity(owner)
                    .remove::<TerrainWaterSuspensionVolumes>();
            }
            continue;
        };
        let supported = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
            .and_then(|water| water.surface_material.configuration.suspension.as_ref())
            .is_some();
        if !supported {
            if projected.contains(owner) {
                commands
                    .entity(owner)
                    .remove::<TerrainWaterSuspensionVolumes>();
            }
            continue;
        }
        let current = projected.get_mut(owner).ok();
        if current.is_some()
            && !changed_regions.contains(owner)
            && !membership
                .iter()
                .any(|change| change.affects_region(source.terrain().id(), owner))
            && !terrain_events.iter().any(|event| {
                event.is_added(source.terrain().id())
                    || event.is_modified(source.terrain().id())
                    || event.is_loaded_with_dependencies(source.terrain().id())
            })
        {
            continue;
        }
        let grid = terrain.canonical_terrain_grid();
        let Some(surface) = source.surface_height(terrain) else {
            continue;
        };
        let spacing = grid.units.cell_size_metres * 4.0;
        let volumes: Box<[_]> = index
            .accepted_points(owner)
            .filter(|point| point.x % 4 == 0 && point.y % 4 == 0)
            .filter_map(|point| {
                let tile = point / 4;
                for y in -1..=1 {
                    for x in -1..=1 {
                        let neighbor = tile.as_ivec2() + IVec2::new(x, y);
                        if neighbor.min_element() < 0
                            || !index.tile_owned_by(neighbor.as_uvec2(), owner)
                        {
                            return None;
                        }
                    }
                }
                let floor = index.slope_tile_center_height(tile, terrain)?;
                (surface > floor).then_some(TerrainWaterSuspensionVolume {
                    origin: Vec3::new(
                        point.x as f32 * grid.units.cell_size_metres,
                        floor,
                        -(point.y as f32) * grid.units.cell_size_metres,
                    ),
                    size: Vec3::new(spacing, surface - floor, spacing),
                })
            })
            .collect();
        let total_m3 = volumes.iter().map(|volume| volume.volume_m3()).sum();
        let next = TerrainWaterSuspensionVolumes { volumes, total_m3 };
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(owner).insert(next);
        }
    }
}

pub(in crate::terrain) fn emit_region_underwater_particles(
    time: Res<Time<Virtual>>,
    phase: Res<State<GamePhase>>,
    terrains: Res<Assets<TerrainAsset>>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    suspension_volumes: Query<&TerrainWaterSuspensionVolumes>,
    regions: TerrainWaterRegionQuery,
    roots: Query<(&ChildOf, &TerrainWaterParticleEffects)>,
    mut suspension_emitters: Query<(
        Entity,
        &ChildOf,
        &mut TerrainWaterSuspensionEmitter,
        &mut ParticleEffectPointEmitter,
    )>,
    mut bubble_emitters: Query<(Entity, &ChildOf, &mut TerrainWaterBubbleEmitter)>,
    mut points: MessageWriter<AppendParticleEffectPoints>,
    mut bubbles: MessageWriter<SpawnParticleEffectEmitter>,
    mut sounds: MessageWriter<
        openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue,
    >,
    mut random: ResMut<NativeVisualRandomStream>,
) {
    if *phase.get() != GamePhase::InGame || time.delta_secs() <= 0.0 {
        return;
    }
    for (entity, parent, mut emitter, mut live) in &mut suspension_emitters {
        let Ok((parent, quality)) = roots.get(parent.parent()) else {
            continue;
        };
        let owner = parent.parent();
        let Some(source) = regions.get(owner) else {
            continue;
        };
        let Some(terrain) = terrains.get(source.terrain()) else {
            continue;
        };
        let Some(water) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let Some(configuration) = water.surface_material.configuration.suspension.as_ref() else {
            continue;
        };
        let Some(emission) = configuration
            .emitters
            .get(emitter.emission_index)
            .or_else(|| configuration.emitters.last())
        else {
            continue;
        };
        let Some(depth) = source
            .depth_ratio(terrain)
            .map(|depth| depth.clamp(0.0, 1.0))
        else {
            continue;
        };
        live.set_size_scale(if depth > 0.75 {
            1.0 + 5.0 * (depth - 0.75)
        } else {
            1.0
        });
        let density = suspension_emitter_density(emission, quality.quality as usize, depth);
        let Ok(volumes) = suspension_volumes.get(owner) else {
            continue;
        };
        let mut emitted = Vec::new();
        if emitter.initial_fill {
            append_initial_suspension_emission(
                &mut emitted,
                &volumes.volumes,
                volumes.total_m3,
                density,
                emitter.particle_radius,
                &mut random,
            );
            emitter.initial_fill = false;
        } else if let Some(count) = live.sampled_live_count() {
            append_suspension_density_top_up(
                &mut emitted,
                &volumes.volumes,
                volumes.total_m3,
                density,
                emitter.particle_radius,
                count,
                &mut random,
            );
        }
        if !emitted.is_empty() {
            points.write(AppendParticleEffectPoints {
                emitter: entity,
                points: emitted.into_boxed_slice(),
            });
        }
    }
    for (emitter_entity, parent, mut emitter) in &mut bubble_emitters {
        let root = parent.parent();
        let Ok((parent, quality)) = roots.get(root) else {
            continue;
        };
        let owner = parent.parent();
        let Some(source) = regions.get(owner) else {
            continue;
        };
        let Some(terrain) = terrains.get(source.terrain()) else {
            continue;
        };
        let Some(index) = indexes
            .iter()
            .find(|index| index.terrain() == source.terrain().id())
        else {
            continue;
        };
        let Some(water) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let configuration = &water.surface_material.configuration;
        let detail = select_bubble_detail(&configuration.bubbles, quality.quality)
            .unwrap_or(&configuration.bubbles_fallback);
        let Some(handle) = detail
            .system
            .as_deref()
            .and_then(|system| terrain.water_particle_effect(system))
        else {
            continue;
        };
        emitter.remaining_seconds -= time.delta_secs();
        if emitter.remaining_seconds > 0.0 {
            continue;
        }
        if let Some(position) = source
            .surface_height(terrain)
            .and_then(|surface| bubble_spawn_point(owner, index, terrain, surface, &mut random))
        {
            bubbles.write(SpawnParticleEffectEmitter {
                particle_effect_asset: handle.clone(),
                emitter_id: emitter.emitter,
                parent_entity: Some(emitter_entity),
                effect_transform: Transform::from_translation(position),
                manual_particle_count: 0,
            });
            if let Some(sound) = detail
                .sound
                .as_deref()
                .filter(|sound| !sound.trim().is_empty())
            {
                use openzt2_zoo_state::audio::audio_playback_message_types::{
                    PlayPositionedAudioCue, PositionedAudioAdmission,
                };
                sounds.write(PlayPositionedAudioCue {
                    cue: openzt2_game_data::AssetId::from_key(&format!(
                        "audio-cue:{}",
                        sound.trim().to_ascii_lowercase()
                    )),
                    world_position: position,
                    selection: emitter_entity.to_bits(),
                    priority: 0,
                    force_looped: false,
                    listener_above_water: false,
                    listener_below_water: true,
                    admission: Some(PositionedAudioAdmission {
                        group: openzt2_game_data::AssetId::from_key("water-bubbles"),
                        maximum_voices: 6,
                        maximum_distance_squared: 300.0,
                    }),
                });
            }
        }
        let timing = detail
            .emitters
            .get(emitter.emission_index)
            .or_else(|| detail.emitters.last());
        emitter.remaining_seconds = bubble_spawn_delay_seconds(
            timing
                .and_then(|timing| timing.small_seconds)
                .unwrap_or(8.0),
            timing.and_then(|timing| timing.big_seconds).unwrap_or(1.0),
            index.render_area_m2(owner, terrain),
            &mut random,
        );
    }
}
