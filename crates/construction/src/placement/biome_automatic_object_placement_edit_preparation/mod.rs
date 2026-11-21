use bevy::ecs::system::SystemParam;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::world_objects::WorldObjectBiomeAutomaticPlacementClass;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

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
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;

#[derive(Component, Debug, Clone)]
pub(super) struct BiomeAutomaticPlacementAssetDependencies {
    biome: openzt2_game_data::AssetId,
    mask: Handle<Image>,
    enabled_object_classes: [bool; 3],
    scene_prefab_handles: Box<[Handle<ScenePrefabAsset>]>,
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct BiomeAutomaticPlacementRandomStream(DeterministicRng);

#[derive(SystemParam)]
pub(super) struct BiomeAutomaticObjectPlacementPreparationParameters<'w, 's> {
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    footprints: Res<'w, ScenePlacementFootprints>,
    images: Res<'w, Assets<Image>>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    previews: Query<
        'w,
        's,
        (
            &'static TerrainBrushPreview,
            Option<&'static TerrainBrushPath>,
            &'static WorldMember,
            &'static BiomeAutomaticPlacementAssetDependencies,
        ),
    >,
    roots: Query<'w, 's, &'static mut BiomeAutomaticPlacementRandomStream, With<WorldRoot>>,
    placement: Res<'w, PlacedObjectFootprintOccupancyIndex>,
    ground_paths: GroundPathPlacementStomp<'w, 's>,
    placed: Query<'w, 's, StompablePlacedObject>,
    allocator: Query<'w, 's, &'static mut PersistentIdAllocator>,
    scratch: ResMut<'w, ObjectPlacementCellCollectionScratch>,
}

pub(super) fn hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews(
    mut commands: Commands,
    seed: Res<ZooSeed>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            Without<BiomeAutomaticPlacementRandomStream>,
        ),
    >,
    previews: Query<(
        Entity,
        &TerrainBrushPreview,
        Option<&BiomeAutomaticPlacementAssetDependencies>,
    )>,
) {
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
    for (entity, preview, retained) in &previews {
        let TerrainBrushKind::Paint { biome, .. } = preview.kind else {
            if retained.is_some() {
                commands
                    .entity(entity)
                    .remove::<BiomeAutomaticPlacementAssetDependencies>();
            }
            continue;
        };
        let enabled_object_classes = [
            preview.paint_trees,
            preview.paint_foliage,
            preview.paint_rocks,
        ];
        if retained.is_some_and(|retained| {
            retained.biome == biome && retained.enabled_object_classes == enabled_object_classes
        }) && !active_definitions.is_changed()
            && !definitions.is_changed()
        {
            continue;
        }
        let Some(definition) = catalogue.find_biome(biome) else {
            commands
                .entity(entity)
                .remove::<BiomeAutomaticPlacementAssetDependencies>();
            continue;
        };
        let Some(mask) = catalogue.texture_image(definition.automatic_placement_mask) else {
            commands
                .entity(entity)
                .remove::<BiomeAutomaticPlacementAssetDependencies>();
            continue;
        };
        let mut candidate_object_identifiers = definition
            .automatic_placement_variations
            .iter()
            .flat_map(|variation| &variation.ranges)
            .flat_map(|range| &range.choices)
            .filter_map(|choice| choice.object)
            .collect::<Vec<_>>();
        candidate_object_identifiers.sort_unstable_by_key(|identifier| identifier.0);
        candidate_object_identifiers.dedup();
        let strong_scene_prefab_handles = candidate_object_identifiers
            .into_iter()
            .filter_map(|identifier| catalogue.find_object(identifier))
            .filter(|object| {
                object
                    .biome_automatic_placement_class
                    .is_some_and(|class| biome_automatic_placement_class_is_enabled(class, preview))
            })
            .filter_map(|object| catalogue.scene(object.prefab))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        commands
            .entity(entity)
            .insert(BiomeAutomaticPlacementAssetDependencies {
                biome,
                mask,
                enabled_object_classes,
                scene_prefab_handles: strong_scene_prefab_handles,
            });
    }
}

#[allow(
    clippy::type_complexity,
    reason = "the query borrows canonical placed-object components directly"
)]
pub(super) fn prepare_biome_automatic_object_placement_edits(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    parameters: BiomeAutomaticObjectPlacementPreparationParameters,
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
        terrain_index,
        terrain_assets,
        chunks,
        previews,
        mut roots,
        placement,
        ground_paths,
        placed,
        mut allocator,
        mut scratch,
    } = parameters;
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((preview, path, member, dependencies)) = previews.get(request.preview) else {
            continue;
        };
        let TerrainBrushKind::Paint { biome, .. } = preview.kind else {
            continue;
        };
        if dependencies.biome != biome
            || !(preview.paint_trees || preview.paint_foliage || preview.paint_rocks)
        {
            continue;
        }
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
        let Some(variation) = biome_definition
            .automatic_placement_variations
            .iter()
            .find(|variation| variation.kind == preview.automatic_placement_variation)
        else {
            continue;
        };
        let mask_offset = IVec2::new(
            i32::from(random.0.next_u32().to_ne_bytes()[0]),
            i32::from(random.0.next_u32().to_ne_bytes()[0]),
        );
        let mut candidate_cells = HashSet::new();
        if let Some(path) = path {
            for dab in &path.0 {
                collect_cells_inside_biome_brush(
                    dab.center,
                    preview.radius_m,
                    &mut candidate_cells,
                );
            }
        } else {
            collect_cells_inside_biome_brush(
                preview.center,
                preview.radius_m,
                &mut candidate_cells,
            );
        }
        let mut candidate_cells = candidate_cells.into_iter().collect::<Vec<_>>();
        candidate_cells.sort_unstable_by_key(|cell| (cell.y, cell.x));

        let mut removed_entities = HashSet::new();
        let mut mutations = Vec::new();
        for cell in candidate_cells.iter().copied() {
            for entity in placement.entities_occupying_cell(cell) {
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
                    biome_automatic_placement_class_is_enabled(class, preview)
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
                    &mut scratch.cells,
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
            let Some(mask_value) =
                sample_world_aligned_biome_automatic_placement_mask(mask, cell + mask_offset)
            else {
                continue;
            };
            let Some(range) = variation
                .ranges
                .iter()
                .find(|range| mask_value <= range.maximum_mask_value)
            else {
                continue;
            };
            let Some(definition_id) =
                select_weighted_biome_automatic_placement_choice(&range.choices, &mut random.0)
            else {
                continue;
            };
            let (Some(object), Some(definition)) = (
                catalogue.find_object(definition_id),
                resolve_object_placeable_definition(catalogue, definition_id),
            ) else {
                continue;
            };
            if !object
                .biome_automatic_placement_class
                .is_some_and(|class| biome_automatic_placement_class_is_enabled(class, preview))
            {
                continue;
            }
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
                scale: Vec3::ONE,
            };
            let Some(prefab) = catalogue.scene(object.prefab) else {
                continue;
            };
            let geometry = footprints.resolve(definition, Some(&prefab));
            let path_overlaps = ground_paths.overlaps(definition, catalogue);
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
            let Some(cells) = collect_occupied_placement_cells_for_transform(
                geometry,
                &transform,
                &mut scratch.cells,
            ) else {
                continue;
            };
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
            let Some(cost) = total_cost.checked_add(definition.price_cents) else {
                continue;
            };
            total_cost = cost;
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
    random: &mut DeterministicRng,
) -> Option<openzt2_game_data::AssetId> {
    let total = choices
        .iter()
        .try_fold(0_u32, |total, choice| total.checked_add(choice.weight))?;
    let draw = random.range_u32(total)?;
    choices
        .iter()
        .scan(0_u32, |maximum, choice| {
            *maximum += choice.weight;
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
