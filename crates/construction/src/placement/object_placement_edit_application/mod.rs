//! Applies prepared object edits after validating the complete ordered batch.

use bevy::{
    ecs::{entity::EntityHashSet, query::QueryData},
    gltf::Gltf,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    world_definitions::object_placement::{
        placement_stomp_rules::PlacementStompAction, PlaceableDefinition,
    },
    AssetId,
};
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex, persistent_id_types::PersistentId,
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    construction::{
        construction_tool_and_placement_policy_types::{
            ConstructionPlacementPolicy, ConstructionTool,
        },
        construction_transaction_types::{EditApplication, RollbackContext},
    },
    placement::{
        object_placement_definition_queries::{
            collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
        },
        object_placement_geometry::ScenePlacementFootprints,
        object_placement_validation::{
            calculate_authored_object_placement_eighth_turns,
            calculate_object_placement_footprint_origin, object_placement_stomp_action,
        },
        placed_object_types::{
            IndexedObjectPlacementCells, PlacedObjectDefinitionReference,
            PlacedObjectFootprintOccupancy,
        },
        placement_occupancy_index::PlacedObjectFootprintQueries,
        placement_preview_types::{PlacementRotationRequest, RelocatingPlacedObject},
        placement_transaction_types::{
            ApplyPreparedObjectPlacementEditRequest, ObjectPlacementCommitted,
            ObjectPlacementEditApplicationAcknowledged, PreparedObjectPlacementEdit,
            PreparedObjectPlacementEditMutationKind,
        },
    },
};

use super::{
    object_placement_edit_direction::object_placement_edit_application_creates_entity,
    placed_object_creation_edit_application::commit_validated_placed_object_creation,
};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct LivePlacement {
    id: &'static PersistentId,
    definition: &'static PlacedObjectDefinitionReference,
    transform: &'static mut Transform,
    footprint: &'static mut PlacedObjectFootprintOccupancy,
}

enum Mutation<'a> {
    Create {
        definition: &'a PlaceableDefinition,
        prefab: &'a ScenePrefabAsset,
        footprint: PlacedObjectFootprintOccupancy,
    },
    Relocate {
        entity: Entity,
        transform: Transform,
        footprint: PlacedObjectFootprintOccupancy,
        forward: bool,
    },
    Remove(Entity),
}

#[derive(Default)]
struct BatchReservations {
    touched: HashSet<(Entity, u64)>,
    moved_or_removed: EntityHashSet,
    new_occupants: HashMap<(Entity, IVec2), Vec<AssetId>>,
}

impl BatchReservations {
    fn apply(&mut self, other: Self) {
        self.touched.extend(other.touched);
        self.moved_or_removed.extend(other.moved_or_removed);
        for (cell, occupants) in other.new_occupants {
            self.new_occupants
                .entry(cell)
                .or_default()
                .extend(occupants);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_prepared_object_placement_edits(
    mut commands: Commands,
    mut requests: MessageReader<ApplyPreparedObjectPlacementEditRequest>,
    mut edits: Query<(
        &mut PreparedObjectPlacementEdit,
        &WorldMember,
        Option<&RollbackContext>,
    )>,
    placement: PlacedObjectFootprintQueries,
    identities: Res<PersistentIdIndex>,
    mut scratch: Local<Vec<IVec2>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    (prefabs, footprints, models): (
        Res<Assets<ScenePrefabAsset>>,
        Res<ScenePlacementFootprints>,
        Res<Assets<Gltf>>,
    ),
    mut placed: Query<LivePlacement>,
    policy: Res<ConstructionPlacementPolicy>,
    mut tool: ResMut<ConstructionTool>,
    mut object_placed: MessageWriter<ObjectPlacementCommitted>,
    mut acknowledged: MessageWriter<ObjectPlacementEditApplicationAcknowledged>,
) {
    let catalogue = active_definitions.get(&definitions);
    let mut reserved = BatchReservations::default();
    for request in requests.read() {
        let accepted = (|| {
            let catalogue = catalogue?;
            let (mut edit, member, rollback) = edits.get_mut(request.transaction).ok()?;
            let root = member.root;
            let forward = matches!(
                request.application,
                EditApplication::InitialCommit | EditApplication::Redo
            ) || (request.application == EditApplication::FailureRollback
                && rollback
                    .is_some_and(|rollback| rollback.failed_application == EditApplication::Undo));
            let (plan, projected) = prepare_batch(
                request.application,
                &edit,
                forward,
                catalogue,
                root,
                &placement,
                &identities,
                &mut scratch,
                &footprints,
                &prefabs,
                &models,
                &placed.as_readonly(),
                &reserved,
            )?;
            reserved.apply(projected);
            for (index, step) in plan {
                let mutation = &mut edit.mutations[index];
                match step {
                    Mutation::Create {
                        definition,
                        prefab,
                        footprint,
                    } => {
                        commit_validated_placed_object_creation(
                            &mut commands,
                            request.transaction,
                            mutation,
                            definition,
                            prefab,
                            root,
                            footprint,
                            policy.repeat_placement,
                            &mut tool,
                            &mut object_placed,
                        );
                    }
                    Mutation::Relocate {
                        entity,
                        transform,
                        footprint,
                        forward,
                    } => {
                        let mut live = placed
                            .get_mut(entity)
                            .expect("placement batch validated the target");
                        live.transform.set_if_neq(transform);
                        live.footprint.set_if_neq(footprint);
                        let cells = if forward {
                            &mutation.cells
                        } else {
                            &mutation.previous_cells
                        };
                        commands
                            .entity(entity)
                            .insert(IndexedObjectPlacementCells(cells.clone()))
                            .remove::<(RelocatingPlacedObject, PlacementRotationRequest)>();
                        if let Some(preview) = mutation.preview.take() {
                            commands.entity(preview).try_despawn();
                        }
                        mutation.applied_entity = Some(entity);
                        if request.application == EditApplication::InitialCommit {
                            *tool = ConstructionTool::Inspect;
                        }
                    }
                    Mutation::Remove(entity) => {
                        commands.entity(entity).try_despawn();
                        mutation.applied_entity = None;
                    }
                }
            }
            Some(())
        })()
        .is_some();
        acknowledged.write(ObjectPlacementEditApplicationAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_batch<'a>(
    application: EditApplication,
    edit: &PreparedObjectPlacementEdit,
    forward: bool,
    catalogue: WorldDefinitionsView<'a>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    ids: &PersistentIdIndex,
    scratch: &mut Vec<IVec2>,
    footprints: &ScenePlacementFootprints,
    prefabs: &'a Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    placed: &Query<LivePlacementReadOnly>,
    reserved: &BatchReservations,
) -> Option<(Vec<(usize, Mutation<'a>)>, BatchReservations)> {
    let count = edit.mutations.len();
    let mut projected = BatchReservations::default();
    let mut plan = Vec::with_capacity(count);
    for offset in 0..count {
        let index = if forward { offset } else { count - 1 - offset };
        let mutation = &edit.mutations[index];
        let id = (root, mutation.entity.0);
        if reserved.touched.contains(&id) || !projected.touched.insert(id) {
            return None;
        }
        let definition = resolve_object_placeable_definition(catalogue, mutation.definition)?;
        let geometry = footprints.resolve(definition, Some(&mutation.prefab));
        let target = ids.get(root, mutation.entity).and_then(|entity| {
            let placed = placed.get(entity).ok()?;
            (placed.id == &mutation.entity && placed.definition.definition == mutation.definition)
                .then_some((entity, placed))
        });
        let places_at = |pose: &Transform, cells: &[IVec2], scratch: &mut Vec<IVec2>| {
            let turns = calculate_authored_object_placement_eighth_turns(definition, pose)?;
            let origin = calculate_object_placement_footprint_origin(geometry, pose, turns)?;
            let expected = collect_occupied_placement_cells_for_transform(geometry, pose, scratch)?;
            (!cells.is_empty() && expected == cells).then_some(PlacedObjectFootprintOccupancy {
                origin,
                eighth_turns: turns,
            })
        };
        let available = |cell: IVec2, ignored: Option<Entity>, projected: &BatchReservations| {
            let blocks = |target| {
                object_placement_stomp_action(definition, catalogue, target)
                    != PlacementStompAction::Allow
            };
            !placement.entities_occupying_cell(root, cell).any(|entity| {
                Some(entity) != ignored
                    && !reserved.moved_or_removed.contains(&entity)
                    && !projected.moved_or_removed.contains(&entity)
                    && blocks(
                        placed
                            .get(entity)
                            .ok()
                            .map(|placed| placed.definition.definition),
                    )
            }) && [reserved, projected].into_iter().all(|batch| {
                batch
                    .new_occupants
                    .get(&(root, cell))
                    .is_none_or(|occupants| !occupants.iter().any(|&id| blocks(Some(id))))
            })
        };
        let step = if mutation.mutation == PreparedObjectPlacementEditMutationKind::Relocate {
            let (entity, live) = target?;
            let (pose, cells) = if forward {
                (mutation.transform, mutation.cells.as_ref())
            } else {
                (
                    mutation.previous_transform?,
                    mutation.previous_cells.as_ref(),
                )
            };
            let previous_pose = if forward {
                mutation.previous_transform?
            } else {
                mutation.transform
            };
            let expected = if forward {
                &mutation.previous_cells
            } else {
                &mutation.cells
            };
            if *live.transform != previous_pose
                || !expected
                    .iter()
                    .all(|cell| placement.cell_contains_entity(root, *cell, entity))
                || !cells
                    .iter()
                    .all(|&cell| available(cell, Some(entity), &projected))
            {
                return None;
            }
            let footprint = places_at(&pose, cells, scratch)?;
            projected.moved_or_removed.insert(entity);
            for &cell in cells {
                projected
                    .new_occupants
                    .entry((root, cell))
                    .or_default()
                    .push(mutation.definition);
            }
            Mutation::Relocate {
                entity,
                transform: pose,
                footprint,
                forward,
            }
        } else if object_placement_edit_application_creates_entity(
            mutation.mutation,
            application,
            target.is_some(),
        ) {
            if target.is_some()
                || !mutation
                    .cells
                    .iter()
                    .all(|&cell| available(cell, None, &projected))
            {
                return None;
            }
            let footprint = places_at(&mutation.transform, &mutation.cells, scratch)?;
            let prefab = prefabs.get(&mutation.prefab)?;
            if first_missing_prefab_collider_model_asset_id(prefab, models).is_some() {
                return None;
            }
            for &cell in mutation.cells.iter() {
                projected
                    .new_occupants
                    .entry((root, cell))
                    .or_default()
                    .push(mutation.definition);
            }
            Mutation::Create {
                definition,
                prefab,
                footprint,
            }
        } else {
            let (entity, live) = target?;
            let _ = places_at(live.transform, &mutation.cells, scratch)?;
            if !mutation
                .cells
                .iter()
                .all(|cell| placement.cell_contains_entity(root, *cell, entity))
            {
                return None;
            }
            projected.moved_or_removed.insert(entity);
            Mutation::Remove(entity)
        };
        plan.push((index, step));
    }
    Some((plan, projected))
}
