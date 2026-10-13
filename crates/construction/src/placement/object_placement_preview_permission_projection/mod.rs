use avian3d::prelude::RayHitData;
use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::ecs::system::SystemParam;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_game_data::world_definitions::object_placement::PlacementConstraints;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::economy::zoo_cash_types::UnlimitedZooCash;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_collision_rebuilding_types::TerrainChunkCollisionSurface;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_cell_indices;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_definition_queries::object_placeable_definition_is_unlocked;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_definition_queries::select_authored_footprint_for_eighth_turns;
use openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementBlockingHeadroomRay;
use openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementHeadroomChunkObservation;
use openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementHeadroomDiagnosticState;
use openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementHeadroomObservation;
use openzt2_zoo_state::placement::object_placement_headroom_diagnostic_types::ObjectPlacementTerrainExemptionObservation;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::object_placement_validation::rotate_object_placement_cell_by_quarter_turns;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;

#[derive(SystemParam)]
pub(super) struct ObjectPlacementHeadroomQueries<'w, 's> {
    spatial_query: SpatialQuery<'w, 's>,
    terrain_index: Res<'w, TerrainIndex>,
    chunks: Query<'w, 's, (&'static TerrainChunk, &'static WorldMember)>,
    surfaces: Query<'w, 's, &'static ChildOf, With<TerrainChunkCollisionSurface>>,
    observed_owners: Query<
        'w,
        's,
        (
            Option<&'static TerrainChunk>,
            Option<&'static WorldMember>,
            Option<&'static ChildOf>,
            Has<TerrainChunkCollisionSurface>,
        ),
    >,
}

impl ObjectPlacementHeadroomQueries<'_, '_> {
    /// Whether `candidate` is another chunk of the indexed chunk's world whose
    /// shared edge row holds `cell`; flattening writes that row in both chunks.
    fn flattens_neighbouring_edge(&self, indexed: Entity, candidate: Entity, cell: Vec2) -> bool {
        let (Ok((indexed, indexed_member)), Ok((candidate, candidate_member))) =
            (self.chunks.get(indexed), self.chunks.get(candidate))
        else {
            return false;
        };
        candidate_member.root == indexed_member.root
            && candidate.coord != indexed.coord
            && candidate.side >= 2
            && terrain_cell_indices(candidate, cell).is_some()
    }

    fn cast_occupied_cell_headroom_ray(
        &self,
        origin: Vec3,
        clearance_m: f32,
        preview: Entity,
        relocation: Option<Entity>,
        flattening_world: Option<Entity>,
        mut observation: Option<&mut ObjectPlacementTerrainExemptionObservation>,
    ) -> Option<RayHitData> {
        let cell = Vec2::new(origin.x, origin.z);
        let flattened_chunk = flattening_world.and_then(|world| {
            let indexed = terrain_chunk_at(&self.terrain_index, cell);
            if let Some(record) = observation.as_deref_mut() {
                record.indexed_entity = indexed;
                record.lookup_stage = "index unavailable";
            }
            let entity = indexed?;
            if let Some(record) = observation.as_deref_mut() {
                record.lookup_stage = "indexed chunk or world member unavailable";
            }
            let (chunk, member) = self.chunks.get(entity).ok()?;
            if let Some(record) = observation.as_deref_mut() {
                record.indexed_chunk = Some(ObjectPlacementHeadroomChunkObservation {
                    coord: chunk.coord,
                    origin: chunk.origin,
                    spacing_m: chunk.spacing_m,
                    side: chunk.side,
                    world: Some(member.root),
                });
                record.lookup_stage = "world mismatch";
            }
            if member.root != world {
                return None;
            }
            if let Some(record) = observation.as_deref_mut() {
                record.lookup_stage = "invalid chunk side";
            }
            if chunk.side < 2 {
                return None;
            }
            let indices = terrain_cell_indices(chunk, cell);
            if let Some(record) = observation.as_deref_mut() {
                record.cell = indices;
                record.lookup_stage = if indices.is_some() {
                    "eligible indexed chunk"
                } else {
                    "cell unavailable"
                };
            }
            indices.map(|_| entity)
        });
        let hit = self.spatial_query.cast_ray_predicate(
            origin,
            Dir3::Y,
            clearance_m,
            true,
            &SpatialQueryFilter::DEFAULT,
            &|hit| {
                hit != preview
                    && Some(hit) != relocation
                    // Flattening replaces all four samples of this occupied
                    // terrain cell in every chunk that stores them, including a
                    // neighbour's shared edge row. Each such heightfield will lie
                    // at placement height, below this unchanged upward ray.
                    && !flattened_chunk.is_some_and(|indexed| {
                        self.surfaces.get(hit).is_ok_and(|parent| {
                            parent.parent() == indexed
                                || self.flattens_neighbouring_edge(indexed, parent.parent(), cell)
                        })
                    })
            },
        );
        // Inspect only the returned blocker, in this same system invocation.
        // Predicate lookups above are retained; no second ray or terrain scan.
        if let (Some(record), Some(hit)) = (observation, hit) {
            record.flattened_chunk = flattened_chunk;
            self.observe_blocking_surface_owner(hit.entity, record);
        }
        hit
    }

    fn observe_blocking_surface_owner(
        &self,
        hit: Entity,
        record: &mut ObjectPlacementTerrainExemptionObservation,
    ) {
        let Ok((_, _, parent, marked)) = self.observed_owners.get(hit) else {
            return;
        };
        record.hit_marked = Some(marked);
        record.hit_parent = parent.map(ChildOf::parent);
        record.parent_matches_indexed = record
            .hit_parent
            .zip(record.indexed_entity)
            .map(|(parent, indexed)| parent == indexed);
        let Some(parent) = record.hit_parent else {
            return;
        };
        let Ok((chunk, member, _, _)) = self.observed_owners.get(parent) else {
            return;
        };
        record.parent_available = true;
        record.parent_world = member.map(|member| member.root);
        record.parent_chunk = chunk.map(|chunk| ObjectPlacementHeadroomChunkObservation {
            coord: chunk.coord,
            origin: chunk.origin,
            spacing_m: chunk.spacing_m,
            side: chunk.side,
            world: record.parent_world,
        });
    }
}

pub(super) fn project_authoritative_facts_onto_object_placement_previews(
    cash: Res<ZooCash>,
    unlimited_cash: Option<Res<UnlimitedZooCash>>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    topology: Res<TopologyIndex>,
    headroom: ObjectPlacementHeadroomQueries,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    mut diagnostics: Option<ResMut<ObjectPlacementHeadroomDiagnosticState>>,
    mut previews: Query<(
        Entity,
        &ConstructionPreview,
        &mut ObjectPlacementPreviewPermissionFacts,
        &ObjectPlacementPrefabSource,
        Option<&PlacedObjectRelocationSource>,
        Option<&WorldMember>,
    )>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let (mode, scenario) = worlds
        .single()
        .map_or((None, None), |(selection, availability)| {
            (Some(selection.mode), availability.map(Ref::into_inner))
        });
    for (entity, preview, mut permission, prefab_source, relocation, member) in &mut previews {
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            continue;
        };
        let constraints = definition.constraints;
        let pose = calculate_authored_object_placement_eighth_turns(definition, &preview.transform)
            .and_then(|turns| {
                calculate_object_placement_footprint_origin(definition, &preview.transform, turns)
                    .map(|origin| (origin, turns))
            });
        let topology_valid = pose.is_some_and(|(origin, turns)| {
            let has_path = !constraints.contains_all(PlacementConstraints::REQUIRE_PATH)
                || authored_object_entrance_definitions(definition)
                    .iter()
                    .any(|entrance| {
                        let local_position = Vec3::from_array(
                            entrance.position_cm.map(|value| f32::from(value) * 0.01),
                        );
                        let local_forward = Vec3::from_array(
                            entrance
                                .forward_snorm
                                .map(|value| f32::from(value) / f32::from(i16::MAX)),
                        )
                        .normalize_or_zero();
                        let world_position = preview.transform.transform_point(local_position);
                        let world_forward = preview
                            .transform
                            .rotation
                            .mul_vec3(local_forward)
                            .normalize_or_zero();
                        [world_position, world_position + world_forward]
                            .into_iter()
                            .map(|point| point.round().as_ivec3())
                            .any(|cell| topology.paths.contains_key(&cell))
                    });
            let has_wall = !constraints.contains_all(PlacementConstraints::REQUIRE_WALL)
                || select_authored_footprint_for_eighth_turns(definition, turns)
                    .iter()
                    .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
                    .any(|cell| {
                        let local =
                            IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                        let cell = origin
                            + rotate_object_placement_cell_by_quarter_turns(local, turns / 2);
                        let cell = IVec3::new(
                            cell.x,
                            preview.transform.translation.round().as_ivec3().y,
                            cell.y,
                        );
                        [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                            .into_iter()
                            .filter_map(|offset| EdgeKey::new(cell, cell + offset))
                            .any(|edge| topology.edges.contains_key(&edge))
                    });
            has_path && has_wall
        });
        let clearance_m = definition.minimum_headroom_metres;
        // The terrain transaction stores centimetres in i16. Outside its
        // representable range, no terrain removal may be assumed.
        let height_cm = preview.transform.translation.y * 100.0;
        let flattening_world = constraints
            .contains_all(PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT)
            .then_some(member)
            .flatten()
            .filter(|_| (f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(&height_cm))
            .map(|member| member.root);
        let mut blocking_ray = None;
        let headroom_valid = clearance_m <= f32::EPSILON
            || pose.is_some_and(|(origin, turns)| {
                select_authored_footprint_for_eighth_turns(definition, turns)
                    .iter()
                    .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
                    .all(|cell| {
                        let local =
                            IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                        let world_cell = origin
                            + rotate_object_placement_cell_by_quarter_turns(local, turns / 2);
                        let cell = world_cell.as_vec2();
                        let ray_origin =
                            Vec3::new(cell.x, preview.transform.translation.y + 0.01, cell.y);
                        let mut exemption = diagnostics
                            .as_ref()
                            .map(|_| ObjectPlacementTerrainExemptionObservation::default());
                        let hit = headroom.cast_occupied_cell_headroom_ray(
                            ray_origin,
                            clearance_m,
                            entity,
                            relocation.map(|source| source.0),
                            flattening_world,
                            exemption.as_mut(),
                        );
                        if let Some(exemption) = exemption {
                            blocking_ray = hit.map(|hit| ObjectPlacementBlockingHeadroomRay {
                                local_cell: local,
                                world_cell,
                                origin: ray_origin,
                                hit,
                                exemption,
                            });
                        }
                        hit.is_none()
                    })
            });
        if let Some(diagnostics) = diagnostics.as_mut() {
            diagnostics.pending = Some(ObjectPlacementHeadroomObservation {
                preview: entity,
                definition: preview.definition,
                transform: preview.transform,
                pose,
                pivot_cm: pose.map(|(_, turns)| {
                    if turns & 1 == 0 {
                        definition.pivot_cm
                    } else {
                        definition.diagonal_pivot_cm
                    }
                }),
                clearance_m,
                headroom_valid,
                flatten_requested: constraints
                    .contains_all(PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT),
                preview_world: member.map(|member| member.root),
                flatten_height_representable: (f32::from(i16::MIN)..=f32::from(i16::MAX))
                    .contains(&height_cm),
                flattening_world,
                blocking_ray,
            });
        }
        let next = ObjectPlacementPreviewPermissionFacts {
            unlocked: object_placeable_definition_is_unlocked(
                catalogue,
                preview.definition,
                mode,
                scenario,
                &unlocks,
            ),
            affordable: unlimited_cash.is_some() || cash.0 .0 >= definition.price_cents,
            topology_valid,
            headroom_valid,
            prefab_ready: prefabs.get(&prefab_source.0).is_some_and(|prefab| {
                first_missing_prefab_collider_model_asset_id(prefab, &models).is_none()
            }),
        };
        if *permission != next {
            *permission = next;
        }
    }
}

#[cfg(test)]
mod tests;
