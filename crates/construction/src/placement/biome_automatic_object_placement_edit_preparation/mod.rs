use super::behavior_target_placement_application::{authored_build_charge, authored_build_terms};
use super::{
    biome_brush_policy::BiomeBrushRules, placement_model_headroom::PlacementModelHeadroom,
};
use crate::construction::construction_transaction_preparation::{
    SubmittedConstruction, SubmittedPreview,
};
use bevy::ecs::system::SystemParam;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementRange;
use openzt2_game_data::world_definitions::world_objects::WorldObjectBiomeAutomaticPlacementClass;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;
use openzt2_game_data::world_definitions::{
    biomes_locations_and_details::BiomeAutomaticPlacementVariationKind,
    object_placement::PlacementConstraints,
};
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    world_spawn::selected_world_identity::SelectedWorldIdentity,
};
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;
use openzt2_zoo_state::{
    economy::zoo_cash_types::{UnlimitedZooCash, ZooCash},
    placement::{
        object_placement_definition_queries::object_placeable_definition_is_unlocked,
        placement_surface_site::PlacementSiteQueries,
    },
    progression::{
        adoption_and_content_availability_types::ScenarioContentAvailability,
        unlock_types::UnlockedCatalogueDefinitionSet,
    },
    terrain::terrain_brush_interaction_state::{ActiveTerrainBrush, TerrainBrushDab},
};

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::TerrainBrushPath;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushPreview;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::placement::ground_path_placement_stomp::{
    GroundPathPlacementStomp, GroundPathStompRemovals,
};
use crate::placement::stomped_object_edit_preparation::{
    object_stomp_prevents_placement_at_cell, prepare_stomped_object_removals, StompablePlacedObject,
};
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(super) struct BiomeAutomaticPlacementAssetDependencies {
    biome: openzt2_game_data::AssetId,
    mask: Handle<Image>,
    scene_prefab_handles: Box<[Handle<ScenePrefabAsset>]>,
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct BiomeAutomaticPlacementRandomStream(DeterministicRng);

#[derive(Component)]
pub(super) struct BiomeMaskStart(IVec2);

/// The range table authored by the active foliage-mix control.
#[derive(Component, Clone, Default, PartialEq, Eq)]
pub struct BiomeAutomaticPlacementRanges(pub Option<Vec<BiomeAutomaticPlacementRange>>);

#[derive(Component, Clone)]
pub(super) struct BiomeObjectDrag {
    brush: TerrainBrushPreview,
    dabs: Vec<TerrainBrushDab>,
    offset: IVec2,
    dependencies: BiomeAutomaticPlacementAssetDependencies,
    ranges: Option<Vec<BiomeAutomaticPlacementRange>>,
}

pub(super) fn begin_biome_object_drag(
    starts: Query<
        (
            Entity,
            &ActiveTerrainBrush,
            &WorldMember,
            Option<&mut BiomeMaskStart>,
        ),
        Added<ActiveTerrainBrush>,
    >,
    mut random: Query<&mut BiomeAutomaticPlacementRandomStream>,
    rules: BiomeBrushRules,
    mut commands: Commands,
) {
    let Some(policy) = rules.policy() else {
        return;
    };
    let mut starts = starts;
    for (entity, brush, world, stored) in &mut starts {
        if !matches!(
            brush.kind,
            TerrainBrushKind::Paint { .. } | TerrainBrushKind::PaintWater { .. }
        ) {
            continue;
        }
        let Ok(mut random) = random.get_mut(world.root) else {
            continue;
        };
        let offset = if policy.random_mask_start {
            IVec2::new(
                i32::from(random.0.next_u32().to_le_bytes()[0]),
                i32::from(random.0.next_u32().to_le_bytes()[0]),
            )
        } else {
            IVec2::ZERO
        };
        if let Some(stored) = stored {
            stored
                .map_unchanged(|start| &mut start.0)
                .set_if_neq(offset);
        } else {
            commands.entity(entity).insert(BiomeMaskStart(offset));
        }
    }
}

pub(super) fn capture_biome_object_drag(
    submissions: Query<
        (Entity, &SubmittedConstruction, &WorldMember),
        Added<SubmittedConstruction>,
    >,
    selection: Query<&BiomeAutomaticPlacementRanges>,
    sources: Query<(
        &TerrainBrushPreview,
        Option<&TerrainBrushPath>,
        &BiomeMaskStart,
        &BiomeAutomaticPlacementAssetDependencies,
    )>,
    mut commands: Commands,
) {
    for (transaction, submission, world) in submissions {
        let Ok((brush, path, start, dependencies)) = sources.get(submission.source) else {
            continue;
        };
        let dabs = path.map_or_else(Vec::new, |path| path.0.clone());
        commands
            .entity(transaction)
            .insert(SubmittedPreview(BiomeObjectDrag {
                brush: *brush,
                dabs,
                offset: start.0,
                dependencies: dependencies.clone(),
                ranges: selection
                    .get(world.root)
                    .ok()
                    .and_then(|selection| selection.0.clone()),
            }));
    }
}

#[derive(SystemParam)]
pub(super) struct BiomeAutomaticObjectPlacementPreparationParameters<'w, 's> {
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    footprints: Res<'w, ScenePlacementFootprints>,
    images: Res<'w, Assets<Image>>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    terrain_indexes: Query<'w, 's, &'static TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    previews: Query<
        'w,
        's,
        (
            &'static SubmittedPreview<BiomeObjectDrag>,
            &'static WorldMember,
        ),
    >,
    roots: Query<'w, 's, &'static mut BiomeAutomaticPlacementRandomStream, With<WorldRoot>>,
    placement: PlacedObjectFootprintQueries<'w, 's>,
    ground_paths: GroundPathPlacementStomp<'w, 's>,
    placed: Query<'w, 's, StompablePlacedObject>,
    allocator: Query<'w, 's, &'static mut PersistentIdAllocator>,
    model_headroom: PlacementModelHeadroom<'w, 's>,
    headroom: super::object_placement_preview_permission_projection::ObjectPlacementHeadroomQueries<
        'w,
        's,
    >,
    sites: PlacementSiteQueries<'w, 's>,
    eligibility: Query<
        'w,
        's,
        (
            &'static ZooCash,
            Option<&'static UnlimitedZooCash>,
            Option<&'static SelectedWorldIdentity>,
            Option<&'static ScenarioContentAvailability>,
            &'static UnlockedCatalogueDefinitionSet,
        ),
    >,
    scratch: Local<'s, Vec<IVec2>>,
}

pub(super) fn hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews(
    mut commands: Commands,
    seed: Single<&ZooSeed>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            Without<BiomeAutomaticPlacementRandomStream>,
        ),
    >,
    mut previews: Query<(
        Entity,
        Ref<TerrainBrushPreview>,
        Option<&mut BiomeAutomaticPlacementAssetDependencies>,
        &WorldMember,
    )>,
    selection: Query<Ref<BiomeAutomaticPlacementRanges>>,
) {
    let seed = seed.into_inner();
    for root in &roots {
        commands
            .entity(root)
            .insert(BiomeAutomaticPlacementRandomStream(
                DeterministicRng::from_entity(
                    *seed,
                    PersistentId(0),
                    RngDomain::BiomeAutomaticPlacement,
                ),
            ));
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let catalogue_changed = active_definitions.is_changed() || definitions.is_changed();
    for (entity, preview, retained, world) in &mut previews {
        let selection = selection.get(world.root).ok();
        if retained.is_some()
            && !preview.is_changed()
            && !catalogue_changed
            && !selection.as_ref().is_some_and(|value| value.is_changed())
        {
            continue;
        }
        let biome = match preview.kind {
            TerrainBrushKind::Paint { biome, .. } | TerrainBrushKind::PaintWater { biome, .. } => {
                biome
            }
            _ => {
                if retained.is_some() {
                    commands
                        .entity(entity)
                        .remove::<BiomeAutomaticPlacementAssetDependencies>();
                }
                continue;
            }
        };
        let Some(definition) = catalogue.find_biome(biome) else {
            if retained.is_some() {
                commands
                    .entity(entity)
                    .remove::<BiomeAutomaticPlacementAssetDependencies>();
            }
            continue;
        };
        let Some(mask) = catalogue.texture_image(definition.automatic_placement_mask) else {
            if retained.is_some() {
                commands
                    .entity(entity)
                    .remove::<BiomeAutomaticPlacementAssetDependencies>();
            }
            continue;
        };
        let water = matches!(preview.kind, TerrainBrushKind::PaintWater { .. });
        let default_ranges = definition
            .automatic_placement_variations
            .iter()
            .filter(|table| {
                if water {
                    table.kind == BiomeAutomaticPlacementVariationKind::Water
                } else {
                    table.kind == preview.automatic_placement_variation
                }
            })
            .flat_map(|variation| &variation.ranges);
        let authored_ranges = selection
            .as_ref()
            .filter(|_| !water)
            .and_then(|value| value.0.as_deref());
        let effective_ranges = default_ranges
            .filter(|_| authored_ranges.is_none())
            .chain(authored_ranges.into_iter().flat_map(|ranges| ranges.iter()));
        let mut candidate_object_identifiers = effective_ranges
            .flat_map(|range| &range.choices)
            .filter_map(|choice| choice.object)
            .collect::<Vec<_>>();
        candidate_object_identifiers.sort_unstable_by_key(|identifier| identifier.0);
        candidate_object_identifiers.dedup();
        let strong_scene_prefab_handles = candidate_object_identifiers
            .into_iter()
            .filter_map(|identifier| catalogue.find_object(identifier))
            .filter(|object| {
                object.biome_automatic_placement_class.is_some_and(|class| {
                    water || biome_automatic_placement_class_is_enabled(class, &preview)
                })
            })
            .filter_map(|object| catalogue.scene(object.prefab))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let dependencies = BiomeAutomaticPlacementAssetDependencies {
            biome,
            mask,
            scene_prefab_handles: strong_scene_prefab_handles,
        };
        if let Some(mut retained) = retained {
            retained.set_if_neq(dependencies);
        } else {
            commands.entity(entity).insert(dependencies);
        }
    }
}

#[allow(
    clippy::type_complexity,
    reason = "the query borrows canonical placed-object components directly"
)]
pub(super) fn prepare_biome_automatic_object_placement_edits(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    params: BiomeAutomaticObjectPlacementPreparationParameters,
    rules: BiomeBrushRules,
    mut prepared: MessageWriter<ObjectPlacementEditPrepared>,
    mut topology_prepared: MessageWriter<TopologyEditPrepared>,
) {
    let BiomeAutomaticObjectPlacementPreparationParameters {
        definitions,
        active_definitions,
        footprints,
        images,
        prefabs,
        models,
        terrain_indexes,
        terrain_assets,
        chunks,
        previews,
        mut roots,
        placement,
        ground_paths,
        placed,
        mut allocator,
        mut scratch,
        mut model_headroom,
        headroom,
        sites,
        eligibility,
    } = params;
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = rules.policy() else {
        return;
    };
    let nothing_weight = rules.nothing_weight_percent(policy);
    for request in requests.read() {
        let Ok((drag, member)) = previews.get(request.transaction) else {
            continue;
        };
        let BiomeObjectDrag {
            brush: preview,
            dabs,
            offset,
            dependencies,
            ranges,
        } = &drag.0;
        let Ok(terrain_index) = terrain_indexes.get(member.root) else {
            continue;
        };
        let (biome, variation_kind, water) = match preview.kind {
            TerrainBrushKind::Paint { biome, .. } => {
                (biome, preview.automatic_placement_variation, false)
            }
            TerrainBrushKind::PaintWater { biome, .. } => {
                (biome, BiomeAutomaticPlacementVariationKind::Water, true)
            }
            _ => continue,
        };
        if dependencies.biome != biome
            || (!water && !(preview.paint_trees || preview.paint_foliage || preview.paint_rocks))
        {
            continue;
        }
        let Ok((cash, unlimited, selection, availability, unlocks)) = eligibility.get(member.root)
        else {
            continue;
        };
        let mode = selection.map(|selection| selection.mode);
        let budget = if unlimited.is_some_and(|unlimited| unlimited.0) {
            i64::MAX
        } else {
            cash.0 .0
        };
        let (Some(biome_definition), Some(mask), Ok(mut random)) = (
            catalogue.find_biome(biome),
            images.get(&dependencies.mask),
            roots.get_mut(member.root),
        ) else {
            continue;
        };
        if dependencies.scene_prefab_handles.iter().any(|handle| {
            prefabs.get(handle).is_none_or(|prefab| {
                first_missing_prefab_collider_model_asset_id(prefab, &models).is_some()
            })
        }) {
            continue;
        }
        let effective_ranges = ranges
            .as_ref()
            .filter(|_| !water)
            .map(Vec::as_slice)
            .or_else(|| {
                biome_definition
                    .automatic_placement_variations
                    .iter()
                    .find(|variation| variation.kind == variation_kind)
                    .map(|variation| variation.ranges.as_slice())
            });
        let Some(effective_ranges) = effective_ranges else {
            continue;
        };
        let mut candidate_cells = Vec::new();
        let mut seen = HashSet::new();
        let mut covered = HashSet::new();
        let mut collect_dab = |centre| {
            collect_cells_inside_biome_brush(centre, preview.radius_m, &mut covered);
            let mut ordered: Vec<_> = covered.drain().collect();
            ordered.sort_unstable_by_key(|cell| (cell.y, cell.x));
            candidate_cells.extend(ordered.into_iter().filter(|&cell| seen.insert(cell)));
        };
        if dabs.is_empty() {
            collect_dab(preview.center);
        } else {
            for dab in dabs {
                collect_dab(dab.center);
            }
        }

        let mut removed_entities = HashSet::new();
        let mut mutations = Vec::new();
        for cell in candidate_cells
            .iter()
            .copied()
            .filter(|_| policy.delete_biome_entities)
        {
            for entity in placement.entities_occupying_cell(member.root, cell) {
                if removed_entities.contains(&entity) {
                    continue;
                }
                let Ok((id, reference, transform, _occupancy, prefab)) = placed.get(entity) else {
                    continue;
                };
                let Some(object) = catalogue.find_object(reference.definition) else {
                    continue;
                };
                let removable_class = object.biome_automatic_placement_class.is_some_and(|class| {
                    water || biome_automatic_placement_class_is_enabled(class, preview)
                });
                if !removable_class
                    || object
                        .properties
                        .contains_all(WorldObjectPropertyFlags::SAVE_RELEVANT)
                {
                    continue;
                }
                let Some(definition) =
                    resolve_object_placeable_definition(catalogue, reference.definition)
                else {
                    continue;
                };
                let Some(cells) = collect_occupied_placement_cells_for_transform(
                    footprints.resolve(definition, Some(&prefab.0)),
                    transform,
                    &mut scratch,
                ) else {
                    continue;
                };
                removed_entities.insert(entity);
                mutations.push(PreparedObjectPlacementMutation {
                    entity: *id,
                    definition: reference.definition,
                    transform: *transform,
                    cells: cells.to_vec().into_boxed_slice(),
                    previous_transform: None,
                    previous_cells: Box::new([]),
                    mutation: PreparedObjectPlacementEditMutationKind::Remove,
                    applied_entity: Some(entity),
                    preview: None,
                    prefab: prefab.0.clone(),
                });
            }
        }

        let mut removed_paths = GroundPathStompRemovals::default();
        let mut created_cells = HashSet::new();
        let mut total_cost = 0_i64;
        for cell in candidate_cells.iter().copied() {
            if !policy.random_mask_start
                && !policy.delete_biome_entities
                && placement
                    .entities_occupying_cell(member.root, cell)
                    .any(|entity| {
                        placed
                            .get(entity)
                            .ok()
                            .and_then(|(_, reference, ..)| {
                                catalogue.find_object(reference.definition)
                            })
                            .is_some_and(|object| object.biomes.contains(&biome))
                    })
            {
                continue;
            }
            let Some(mask_value) =
                sample_world_aligned_biome_automatic_placement_mask(mask, cell + *offset)
            else {
                continue;
            };
            let Some(range) = effective_ranges
                .iter()
                .find(|range| mask_value <= range.maximum_mask_value)
            else {
                continue;
            };
            let Some(definition_id) = select_weighted_biome_automatic_placement_choice(
                &range.choices,
                nothing_weight,
                &mut random.0,
            ) else {
                continue;
            };
            let (Some(object), Some(definition)) = (
                catalogue.find_object(definition_id),
                resolve_object_placeable_definition(catalogue, definition_id),
            ) else {
                continue;
            };
            if !object.biome_automatic_placement_class.is_some_and(|class| {
                water || biome_automatic_placement_class_is_enabled(class, preview)
            }) {
                continue;
            }
            if water
                && (!rules.object_detail_allows(definition.minimum_object_detail)
                    || !definition.surface_policy.water
                    || !object
                        .properties
                        .contains_all(WorldObjectPropertyFlags::WATER_PLACEABLE))
            {
                continue;
            }
            if mode != Some(WorldSessionMode::Freeform)
                && !object_placeable_definition_is_unlocked(
                    catalogue,
                    definition_id,
                    mode,
                    availability,
                    unlocks,
                )
            {
                continue;
            }
            let Some((_, terms)) = authored_build_terms(catalogue, definition_id) else {
                continue;
            };
            let Some(amount) =
                authored_build_charge(&terms, object.price_cents, || Some(random.0.next_u32()))
            else {
                continue;
            };
            let Some(next_cost) = total_cost
                .checked_add(amount.0)
                .filter(|&cost| cost <= budget)
            else {
                continue;
            };
            let position = Vec2::new(
                cell.x as f32 + random.0.unit_f32() - 0.5,
                cell.y as f32 + random.0.unit_f32() - 0.5,
            );
            let Some(point) = terrain_chunk_at(&terrain_index, position).and_then(|entity| {
                let (chunk, edited) = chunks.get(entity).ok()?;
                let asset = terrain_assets.get(&chunk.asset)?;
                sample_terrain(chunk, asset, edited, position)
            }) else {
                continue;
            };
            let eighth_turns = random.0.range_u32(8).unwrap_or_default() as u8;
            let transform = Transform {
                translation: Vec3::new(position.x, point.height_m, position.y),
                rotation: Quat::from_rotation_y(
                    f32::from(eighth_turns) * core::f32::consts::FRAC_PI_4,
                ),
                scale: Vec3::splat(object.prefab_scale),
            };
            let Some(prefab) = catalogue.scene(object.prefab) else {
                continue;
            };
            let Some(scene) = prefabs.get(&prefab) else {
                continue;
            };
            model_headroom.request_models(definition, scene);
            let Some(clearance) = model_headroom.clearance(definition, Some(scene), &transform)
            else {
                continue;
            };
            if definition
                .constraints
                .contains_all(PlacementConstraints::REQUIRE_PATH)
                || definition
                    .constraints
                    .contains_all(PlacementConstraints::REQUIRE_WALL)
                || sites
                    .validate(
                        definition.surface_policy,
                        member.root,
                        None,
                        transform.translation,
                    )
                    .is_err()
            {
                continue;
            }
            let geometry = footprints.resolve(definition, Some(&prefab));
            let path_overlaps = ground_paths.overlaps(member.root, definition, catalogue);
            let validity = validate_object_placement_footprint_cells(
                geometry,
                geometry.cells(eighth_turns),
                &transform,
                |position| {
                    let entity = terrain_chunk_at(&terrain_index, position)?;
                    let (chunk, edited) = chunks.get(entity).ok()?;
                    let asset = terrain_assets.get(&chunk.asset)?;
                    sample_terrain(chunk, asset, edited, position)
                },
                |occupied_cell| {
                    created_cells.contains(&occupied_cell)
                        || path_overlaps.prevents_placement(occupied_cell)
                        || object_stomp_prevents_placement_at_cell(
                            occupied_cell,
                            definition,
                            catalogue,
                            member.root,
                            &placement,
                            &placed,
                            |entity| removed_entities.contains(&entity),
                        )
                },
                ObjectPlacementAuthoritativeFacts {
                    habitat: None,
                    unlocked: true,
                    affordable: true,
                    topology_valid: true,
                    headroom_valid: true,
                    surface_valid: true,
                },
            );
            if !matches!(validity, PlacementValidity::Valid { .. }) {
                continue;
            }
            let Some(cells) =
                collect_occupied_placement_cells_for_transform(geometry, &transform, &mut scratch)
            else {
                continue;
            };
            if clearance > f32::EPSILON
                && cells.iter().any(|cell| {
                    let origin =
                        Vec3::new(cell.x as f32, transform.translation.y + 0.01, cell.y as f32);
                    headroom
                        .cast_occupied_cell_headroom_ray(
                            origin,
                            clearance,
                            Entity::PLACEHOLDER,
                            None,
                            None,
                            None,
                        )
                        .is_some()
                })
            {
                continue;
            }
            if prefabs.get(&prefab).is_none_or(|scene| {
                (definition.automatic_footprint
                    && scene
                        .canonical_scene_prefab_document()
                        .automatic_placement_bounds_xz
                        .is_some()
                    && !footprints.has_scene_bounds(&prefab))
                    || first_missing_prefab_collider_model_asset_id(scene, &models).is_some()
            }) {
                continue;
            }
            let cells = cells.to_vec().into_boxed_slice();
            let Some(removals) = prepare_stomped_object_removals(
                &cells,
                definition,
                catalogue,
                member.root,
                &placement,
                &placed,
                |entity| removed_entities.contains(&entity),
            ) else {
                continue;
            };
            let Some(entity) = allocator
                .get_mut(member.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                continue;
            };
            total_cost = next_cost;
            removed_paths.collect(&path_overlaps, &cells);
            removed_entities.extend(removals.iter().filter_map(|removal| removal.applied_entity));
            mutations.extend(removals);
            created_cells.extend(cells.iter().copied());
            mutations.push(PreparedObjectPlacementMutation {
                entity,
                definition: definition_id,
                transform,
                cells,
                previous_transform: None,
                previous_cells: Box::new([]),
                mutation: PreparedObjectPlacementEditMutationKind::Create,
                applied_entity: None,
                preview: None,
                prefab,
            });
        }
        if mutations.is_empty() {
            continue;
        }
        removed_paths.prepare(request.transaction, &mut commands, &mut topology_prepared);
        commands
            .entity(request.transaction)
            .insert(PreparedObjectPlacementEdit {
                mutations: mutations.into_boxed_slice(),
            });
        prepared.write(ObjectPlacementEditPrepared {
            transaction: request.transaction,
            cost: Money(total_cost),
        });
    }
}

fn collect_cells_inside_biome_brush(center: Vec2, radius: f32, output: &mut HashSet<IVec2>) {
    let minimum = (center - Vec2::splat(radius)).floor().as_ivec2();
    let maximum = (center + Vec2::splat(radius)).ceil().as_ivec2();
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let cell = IVec2::new(x, y);
            if cell.as_vec2().distance_squared(center) <= radius * radius {
                output.insert(cell);
            }
        }
    }
}

fn sample_world_aligned_biome_automatic_placement_mask(mask: &Image, cell: IVec2) -> Option<u8> {
    let size = mask.texture_descriptor.size;
    let x = cell.x.rem_euclid(i32::try_from(size.width).ok()?) as u32;
    let y = cell.y.rem_euclid(i32::try_from(size.height).ok()?) as u32;
    mask.pixel_bytes(UVec3::new(x, y, 0)).ok()?.first().copied()
}

fn select_weighted_biome_automatic_placement_choice(
    choices: &[openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementChoice],
    nothing_weight_percent: u32,
    random: &mut DeterministicRng,
) -> Option<openzt2_game_data::AssetId> {
    let weight = |choice: &openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementChoice| {
        if choice.object.is_some() { choice.weight }
        else { u32::try_from(u64::from(choice.weight) * u64::from(nothing_weight_percent) / 100).unwrap_or(u32::MAX) }
    };
    let total = choices
        .iter()
        .try_fold(0_u32, |total, choice| total.checked_add(weight(choice)))?;
    let draw = random.range_u32(total)?;
    choices
        .iter()
        .scan(0_u32, |maximum, choice| {
            *maximum += weight(choice);
            Some((*maximum, choice))
        })
        .find(|(maximum, _)| draw < *maximum)
        .and_then(|(_, choice)| choice.object)
}

const fn biome_automatic_placement_class_is_enabled(
    class: WorldObjectBiomeAutomaticPlacementClass,
    preview: &TerrainBrushPreview,
) -> bool {
    match class {
        WorldObjectBiomeAutomaticPlacementClass::Tree => preview.paint_trees,
        WorldObjectBiomeAutomaticPlacementClass::Plant => preview.paint_foliage,
        WorldObjectBiomeAutomaticPlacementClass::Rock => preview.paint_rocks,
    }
}
