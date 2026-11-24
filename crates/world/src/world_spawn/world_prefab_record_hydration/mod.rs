use avian3d::prelude::RigidBody;
use bevy::asset::LoadState;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::gltf::Gltf;
use bevy::prelude::Assets;
use bevy::prelude::ChildOf;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Quat;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::Transform;
use bevy::prelude::Vec3;
use bevy::prelude::Visibility;
use bevy::prelude::{AssetServer, Component, Handle, Or, With, Without};
use openzt2_game_data::world_definitions::world_objects::WorldObjectInformationViewClass;
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;
use openzt2_game_data::world_scenario::StartingZooSpawnEntityFlags;
use openzt2_game_data::world_scenario::{StartingZooEntityTransform, StartingZooSpawnEntityRecord};
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::physics::fitting_surface_types::TerrainFittingSurface;
use openzt2_interface_state::information::entity_selection_types::Inspectable;

use crate::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::zoo_entrance_from_authored_gate_anchor;

use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::starting_animal_hydration::PendingStartingAnimal;
use openzt2_world_state::world_spawn::world_hydration_types::StartingAnimalPendingFor;
use openzt2_world_state::world_spawn::world_load_completion_marker::{
    ImportedPersistentIdsReserved, StartingPrefabsHydrated,
};

const MAXIMUM_PREFAB_WORK_IN_FLIGHT: usize = 128;

#[derive(Component)]
#[relationship(relationship_target = StartingPrefabRows)]
pub(super) struct StartingPrefabRow {
    #[relationship]
    root: Entity,
    index: usize,
}

#[derive(Component)]
#[relationship_target(relationship = StartingPrefabRow, linked_spawn)]
pub(super) struct StartingPrefabRows(Vec<Entity>);

#[derive(Component)]
pub(super) struct ApplyStartingAnimalHiddenVisibility;

pub(super) fn admit_starting_prefab_rows(
    mut commands: Commands,
    loads: Query<
        (Entity, &WorldHydration, Option<&StartingPrefabRows>),
        (
            With<ImportedPersistentIdsReserved>,
            Without<StartingPrefabsHydrated>,
            Without<WorldLoadFailure>,
        ),
    >,
) {
    for (root, progress, pending) in &loads {
        let (committed, total) = progress.prefab_loading_progress();
        let first = committed + pending.map_or(0, RelationshipTarget::len);
        let end = total.min(committed + MAXIMUM_PREFAB_WORK_IN_FLIGHT);
        for index in first..end {
            commands.spawn(StartingPrefabRow { root, index });
        }
    }
}

#[derive(Component)]
pub(super) enum StartingPrefabDependencies {
    Animal,
    Nonvisual {
        object: AssetId,
        named_prefabs: Vec<(AssetId, Handle<ScenePrefabAsset>)>,
    },
    Scene {
        id: AssetId,
        handle: Handle<ScenePrefabAsset>,
        object: Option<AssetId>,
    },
}

#[derive(Component)]
pub(super) struct StartingPrefabDependenciesReady;

pub(super) fn demand_starting_prefab_dependencies(
    mut commands: Commands,
    rows: Query<
        (Entity, &StartingPrefabRow),
        (
            Without<StartingPrefabDependencies>,
            Without<WorldLoadFailure>,
        ),
    >,
    worlds: Query<
        (&SelectedWorldIdentity, &SelectedWorldDocumentHandles),
        Without<WorldLoadFailure>,
    >,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    asset_server: Res<AssetServer>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    if species_index.catalogue_load_is_pending(&asset_server) {
        return;
    }
    let species = species_index.get(&species_assets);
    for (job, row) in &rows {
        let Ok((selection, documents)) = worlds.get(row.root) else {
            continue;
        };
        let Some(document) = scenarios.get(&documents.start) else {
            commands
                .entity(job)
                .insert_if_new(WorldLoadFailure::MissingScenario);
            continue;
        };
        let Some(record) = document
            .document
            .find_starting_zoo(selection.start)
            .and_then(|start| start.entities.get(row.index))
        else {
            commands
                .entity(job)
                .insert_if_new(WorldLoadFailure::InvalidRecord(row.index as u32));
            continue;
        };
        let definition = AssetId(record.definition.0);
        let dependencies =
            if species.is_some_and(|species| species.find_variant(definition).is_some()) {
                Ok(StartingPrefabDependencies::Animal)
            } else {
                resolve_starting_object_dependencies(catalogue, document, record, definition)
            };
        match dependencies {
            Ok(dependencies) => {
                commands.entity(job).insert(dependencies);
            }
            Err(reason) if active_definitions.is_complete() => {
                commands.entity(job).insert_if_new(reason);
            }
            Err(_) => {}
        }
    }
}

fn resolve_starting_object_dependencies(
    catalogue: WorldDefinitionsView<'_>,
    document: &WorldScenarioDocumentAsset,
    record: &StartingZooSpawnEntityRecord,
    definition: AssetId,
) -> Result<StartingPrefabDependencies, WorldLoadFailure> {
    let object = catalogue
        .find_object(definition)
        .map(|object| object.id)
        .or_else(|| catalogue.find_facility(definition).map(|role| role.object))
        .or_else(|| catalogue.find_staff(definition).map(|role| role.object))
        .or_else(|| catalogue.find_guest(definition).map(|role| role.object))
        .or_else(|| catalogue.find_path(definition).map(|role| role.object))
        .or_else(|| catalogue.find_fence(definition).map(|role| role.object));
    let staff_prefab = catalogue.find_staff(definition)
        .or_else(|| catalogue.find_staff_by_object(definition))
        .and_then(|role| {
            openzt2_zoo_state::staff::staff_presentation_variant_selection::staff_presentation_variant(
                role,
                record.persistent_id,
                catalogue,
            )
        })
        .map(|(variant, _)| variant.prefab);
    let object_definition = object.and_then(|id| catalogue.find_object(id));
    let id = if record.prefab != AssetId::default() {
        record.prefab
    } else {
        staff_prefab
            .or_else(|| object_definition.map(|object| object.prefab))
            .ok_or(WorldLoadFailure::MissingDependency(definition))?
    };
    if id != AssetId::default() {
        let handle = document
            .scene(id)
            .cloned()
            .or_else(|| catalogue.scene(id))
            .ok_or(WorldLoadFailure::MissingDependency(id))?;
        return Ok(StartingPrefabDependencies::Scene { id, handle, object });
    }
    let object = object_definition.ok_or(WorldLoadFailure::MissingDependency(definition))?;
    let named_prefabs = object
        .named_physical_presentations
        .iter()
        .filter(|presentation| presentation.required && presentation.prefab != AssetId::default())
        .map(|presentation| {
            let id = presentation.prefab;
            catalogue
                .scene(id)
                .map(|handle| (id, handle))
                .ok_or(WorldLoadFailure::MissingDependency(id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StartingPrefabDependencies::Nonvisual {
        object: object.id,
        named_prefabs,
    })
}

pub(super) fn mark_ready_starting_prefab_dependencies(
    mut commands: Commands,
    rows: Query<
        (Entity, &StartingPrefabDependencies),
        (
            Without<StartingPrefabDependenciesReady>,
            Without<WorldLoadFailure>,
        ),
    >,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    asset_server: Res<AssetServer>,
) {
    for (job, dependencies) in &rows {
        let scenes = match dependencies {
            StartingPrefabDependencies::Animal => &[][..],
            StartingPrefabDependencies::Scene { id, handle, .. } => {
                mark_starting_prefab_row_ready(
                    &mut commands,
                    job,
                    scene_and_colliders_ready(*id, handle, &prefabs, &models, &asset_server),
                );
                continue;
            }
            StartingPrefabDependencies::Nonvisual { named_prefabs, .. } => named_prefabs.as_slice(),
        };
        let mut readiness = Ok(true);
        for (id, handle) in scenes {
            match scene_and_colliders_ready(*id, handle, &prefabs, &models, &asset_server) {
                Ok(false) => readiness = Ok(false),
                Ok(true) => {}
                Err(reason) => {
                    readiness = Err(reason);
                    break;
                }
            }
        }
        mark_starting_prefab_row_ready(&mut commands, job, readiness);
    }
}

fn mark_starting_prefab_row_ready(
    commands: &mut Commands,
    job: Entity,
    readiness: Result<bool, WorldLoadFailure>,
) {
    match readiness {
        Ok(true) => {
            commands.entity(job).insert(StartingPrefabDependenciesReady);
        }
        Ok(false) => {}
        Err(reason) => {
            commands.entity(job).insert_if_new(reason);
        }
    }
}

fn scene_and_colliders_ready(
    id: AssetId,
    handle: &Handle<ScenePrefabAsset>,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    asset_server: &AssetServer,
) -> Result<bool, WorldLoadFailure> {
    let Some(prefab) = prefabs.get(handle) else {
        return if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
            Err(WorldLoadFailure::MissingDependency(id))
        } else {
            Ok(false)
        };
    };
    if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, models) {
        return if prefab
            .loaded_model_asset_handle(model)
            .is_none_or(|handle| {
                matches!(asset_server.load_state(handle.id()), LoadState::Failed(_))
            }) {
            Err(WorldLoadFailure::MissingDependency(model))
        } else {
            Ok(false)
        };
    }
    Ok(true)
}

pub(super) fn commit_ready_starting_prefab_rows_in_source_order(
    mut commands: Commands,
    mut loads: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
            &mut WorldHydration,
            Option<&StartingPrefabRows>,
        ),
        (
            With<ImportedPersistentIdsReserved>,
            Without<StartingPrefabsHydrated>,
            Without<WorldLoadFailure>,
        ),
    >,
    ready: Query<
        (
            Option<&StartingPrefabDependencies>,
            Option<&WorldLoadFailure>,
            &StartingPrefabRow,
        ),
        Or<(
            With<StartingPrefabDependenciesReady>,
            With<WorldLoadFailure>,
        )>,
    >,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (root, selection, documents, mut progress, pending) in &mut loads {
        let Some(start) = scenarios
            .get(&documents.start)
            .and_then(|asset| asset.document.find_starting_zoo(selection.start))
        else {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::MissingScenario);
            continue;
        };
        let (first, _) = progress.prefab_loading_progress();
        let mut ready_prefix = [None; MAXIMUM_PREFAB_WORK_IN_FLIGHT];
        if let Some(pending) = pending {
            for (job, (dependencies, failure, row)) in pending
                .iter()
                .filter_map(|job| ready.get(job).ok().map(|values| (job, values)))
            {
                if let Some(slot) = row
                    .index
                    .checked_sub(first)
                    .and_then(|offset| ready_prefix.get_mut(offset))
                {
                    *slot = Some((job, dependencies, failure));
                }
            }
        }
        for (index, entry) in (first..).zip(ready_prefix) {
            let Some((job, dependencies, failure)) = entry else {
                break;
            };
            if let Some(&reason) = failure {
                commands.entity(root).insert_if_new(reason);
                break;
            }
            let Some(dependencies) = dependencies else {
                break;
            };
            let Some(record) = start.entities.get(index) else {
                commands
                    .entity(root)
                    .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
                break;
            };
            let parent = if matches!(dependencies, StartingPrefabDependencies::Animal)
                || record.parent == u32::MAX
            {
                None
            } else {
                let Some(parent) =
                    progress.spawned_world_prefab_root_at_record_index(record.parent)
                else {
                    commands
                        .entity(root)
                        .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
                    break;
                };
                Some(parent)
            };
            match spawn_starting_prefab_row(
                &mut commands,
                root,
                record,
                parent,
                dependencies,
                catalogue,
                &prefabs,
            ) {
                Ok(entity) => {
                    progress.record_world_prefab_spawn_completion(entity);
                    commands.entity(job).despawn();
                }
                Err(reason) => {
                    commands.entity(root).insert_if_new(reason);
                    break;
                }
            }
        }
        let (committed, total) = progress.prefab_loading_progress();
        if committed == total {
            commands.entity(root).insert(StartingPrefabsHydrated);
        }
    }
}

fn spawn_starting_prefab_row(
    commands: &mut Commands,
    root: Entity,
    record: &StartingZooSpawnEntityRecord,
    parent: Option<Entity>,
    dependencies: &StartingPrefabDependencies,
    catalogue: WorldDefinitionsView<'_>,
    prefabs: &Assets<ScenePrefabAsset>,
) -> Result<Entity, WorldLoadFailure> {
    let definition = record.definition;
    let transform = transform_from_world(&record.transform);
    let visible = record
        .flags
        .contains_all(StartingZooSpawnEntityFlags::VISIBLE);
    let id = PersistentId(record.persistent_id);
    match dependencies {
        StartingPrefabDependencies::Animal => {
            let mut animal = commands.spawn((
                WorldMember { root },
                DefinitionId(definition),
                id,
                transform,
                if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                PendingStartingAnimal {
                    variant: definition,
                    initial_needs: record.initial_needs.clone(),
                },
                StartingAnimalPendingFor(root),
            ));
            if !visible {
                animal.insert(ApplyStartingAnimalHiddenVisibility);
            }
            Ok(animal.id())
        }
        StartingPrefabDependencies::Nonvisual {
            object,
            named_prefabs,
        } => {
            let object = catalogue
                .find_object(*object)
                .ok_or(WorldLoadFailure::MissingDependency(*object))?;
            let mut named_presentations = Vec::new();
            for presentation in object
                .named_physical_presentations
                .iter()
                .filter(|item| item.required)
            {
                let handle = if presentation.prefab == AssetId::default() {
                    None
                } else {
                    Some(
                        named_prefabs
                            .iter()
                            .find(|(id, _)| *id == presentation.prefab)
                            .map(|(_, handle)| handle.clone())
                            .ok_or(WorldLoadFailure::MissingDependency(presentation.prefab))?,
                    )
                };
                named_presentations.push((presentation, handle));
            }
            let mut entity = commands.spawn((
                WorldMember { root },
                DefinitionId(definition),
                id,
                Inspectable { definition },
                transform,
                if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            if let Some(parent) = parent {
                entity.insert(ChildOf(parent));
            }
            let entity = entity.id();
            crate::world_spawn::named_physical_presentation_projection::spawn_required_named_physical_presentations(
                commands, entity, named_presentations,
            );
            Ok(entity)
        }
        StartingPrefabDependencies::Scene {
            id: prefab_id,
            handle,
            object,
        } => {
            let prefab = prefabs
                .get(handle)
                .ok_or(WorldLoadFailure::MissingDependency(*prefab_id))?;
            let body = object
                .and_then(|object| catalogue.find_object(object))
                .map_or(RigidBody::Static, |object| {
                    if matches!(
                        object.kind,
                        WorldObjectKind::Staff
                            | WorldObjectKind::Guest
                            | WorldObjectKind::Animal
                            | WorldObjectKind::Vehicle
                    ) || catalogue
                        .find_placeable(object.id)
                        .is_some_and(|placeable| placeable.moving_footprint)
                    {
                        RigidBody::Dynamic
                    } else {
                        RigidBody::Static
                    }
                });
            let entity = spawn_loaded_scene_prefab_as_world_instance(
                commands,
                prefab,
                handle.clone(),
                root,
                definition,
                id,
                transform,
                visible,
                parent,
                body,
            );
            if let Some(object) = catalogue
                .find_object(definition)
                .filter(|object| object.terrain_fitted)
            {
                commands.entity(entity).insert(TerrainFittingSurface {
                    normal_tolerance: 0.0,
                    height_offset_m: object.terrain_fit_height_offset_m,
                    water_offset_m: object.terrain_fit_water_offset_m,
                });
            }
            if is_zoo_entrance(catalogue, definition) {
                let anchor = transform.mul_transform(transform_from_authored(
                    &prefab.canonical_scene_prefab_document().entities[0].transform,
                ));
                commands
                    .entity(entity)
                    .insert(zoo_entrance_from_authored_gate_anchor(anchor.translation));
            }
            Ok(entity)
        }
    }
}

fn is_zoo_entrance(definitions: WorldDefinitionsView<'_>, id: AssetId) -> bool {
    definitions
        .find_object(id)
        .and_then(|definition| definition.view_class.as_ref())
        .is_some_and(|class| matches!(class, WorldObjectInformationViewClass::Entrance))
}

fn transform_from_world(value: &StartingZooEntityTransform) -> Transform {
    Transform {
        translation: Vec3::from_array(value.translation_m),
        rotation: Quat::from_array(value.rotation_xyzw),
        scale: Vec3::from_array(value.scale),
    }
}
