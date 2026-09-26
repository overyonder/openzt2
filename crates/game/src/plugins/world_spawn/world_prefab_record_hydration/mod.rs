use avian3d::prelude::RigidBody;
use bevy::{
    asset::LoadState,
    gltf::Gltf,
    prelude::{
        AssetServer, Assets, ChildOf, Commands, Entity, Quat, Query, Res, ResMut, Transform, Vec3,
        Visibility,
    },
};
use openzt2_game_data::{
    world_definitions::world_objects::{WorldObjectInformationViewClass, WorldObjectKind},
    world_scenario::{StartingZooEntityTransform, StartingZooSpawnEntityFlags},
    AssetId,
};

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::physics::fitting_surface_types::TerrainFittingSurface;
use crate::plugins::information::entity_selection_types::Inspectable;

use super::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    prefab_transform_conversion::transform_from_authored,
    prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
    world_hydration_types::WorldHydration,
    world_load_failure::WorldLoadFailure,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::{DefinitionId, WorldMember},
    zoo_entrance_anchor_synchronization::zoo_entrance_from_authored_gate_anchor,
};

const WORLD_PREFAB_RECORD_HYDRATION_BATCH_SIZE: usize = 128;

pub(super) fn hydrate_bounded_world_prefab_record_batch(
    mut commands: Commands,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    models: Res<Assets<Gltf>>,
    asset_server: Res<AssetServer>,
    allocator: Option<Res<PersistentIdAllocator>>,
    mut pending: Query<(Entity, &mut WorldHydration)>,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::WorldPrefabRecordHydration);
    let Some(allocator) = allocator else {
        return;
    };
    let Ok((root, mut pending)) = pending.single_mut() else {
        return;
    };
    if !pending.world_prefab_record_hydration_is_pending() {
        return;
    }
    if !allocator.owns_world(root) {
        let next_record = pending.next_world_prefab_record_index() as u32;
        pending.record_failure(WorldLoadFailure::InvalidRecord(next_record));
        return;
    }
    let Some(start_asset) = scenarios.get(pending.starting_zoo_document_asset_handle()) else {
        pending.record_failure(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(active_catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = &start_asset.document;
    let end =
        pending.bounded_world_prefab_record_batch_end(WORLD_PREFAB_RECORD_HYDRATION_BATCH_SIZE);

    'records: while pending.next_world_prefab_record_index() < end {
        let local_index = pending.next_world_prefab_record_index();
        let Some(record) = catalog
            .find_starting_zoo(pending.starting_zoo_id())
            .and_then(|start| start.entities.get(local_index))
        else {
            pending.record_failure(WorldLoadFailure::InvalidRecord(local_index as u32));
            break;
        };
        let definition_id = AssetId(record.definition.0);
        let object_definition_id = active_catalogue
            .find_object(definition_id)
            .map(|record| record.id)
            .or_else(|| {
                active_catalogue
                    .find_facility(definition_id)
                    .map(|record| record.object)
            })
            .or_else(|| {
                active_catalogue
                    .find_staff(definition_id)
                    .map(|record| record.object)
            })
            .or_else(|| {
                active_catalogue
                    .find_guest(definition_id)
                    .map(|record| record.object)
            })
            .or_else(|| {
                active_catalogue
                    .find_path(definition_id)
                    .map(|record| record.object)
            })
            .or_else(|| {
                active_catalogue
                    .find_fence(definition_id)
                    .map(|record| record.object)
            });
        let prefab_id = if record.prefab != AssetId::default() {
            AssetId(record.prefab.0)
        } else {
            let Some(prefab) = object_definition_id.and_then(|object| {
                active_catalogue
                    .find_object(object)
                    .map(|record| record.prefab)
            }) else {
                break;
            };
            prefab
        };
        let parent = match record.parent {
            u32::MAX => None,
            index => pending.spawned_world_prefab_root_at_record_index(index),
        };
        if record.parent != u32::MAX && parent.is_none() {
            pending.record_failure(WorldLoadFailure::InvalidRecord(local_index as u32));
            break;
        }
        // A declared object without a scene still owns identity and gameplay state.
        if prefab_id == AssetId::default() {
            let object = object_definition_id
                .and_then(|id| active_catalogue.find_object(id))
                .expect("a missing scene requires a declared nonvisual object");
            let mut named_presentations = Vec::new();
            for presentation in object
                .named_physical_presentations
                .iter()
                .filter(|presentation| presentation.required)
            {
                if presentation.prefab == AssetId::default() {
                    named_presentations.push((presentation, None));
                    continue;
                }
                if pending
                    .retained_pending_named_prefab(presentation.prefab)
                    .is_none()
                {
                    let Some(handle) = active_catalogue.scene(presentation.prefab) else {
                        pending.record_failure(WorldLoadFailure::MissingDependency(
                            presentation.prefab,
                        ));
                        break 'records;
                    };
                    pending.retain_pending_named_prefab(presentation.prefab, handle);
                }
                let handle = pending
                    .retained_pending_named_prefab(presentation.prefab)
                    .expect("required named scene was retained")
                    .clone();
                let Some(prefab) = prefabs.get(&handle) else {
                    if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
                        pending.record_failure(WorldLoadFailure::MissingDependency(
                            presentation.prefab,
                        ));
                    }
                    break 'records;
                };
                if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, &models) {
                    if prefab.loaded_model_asset_handle(model).is_none_or(|model| {
                        matches!(asset_server.load_state(model.id()), LoadState::Failed(_))
                    }) {
                        pending.record_failure(WorldLoadFailure::MissingDependency(model));
                    }
                    break 'records;
                }
                named_presentations.push((presentation, Some(handle)));
            }

            let mut entity = commands.spawn((
                WorldMember { root },
                DefinitionId(definition_id),
                PersistentId(record.persistent_id),
                Inspectable {
                    definition: definition_id,
                },
                transform_from_world(&record.transform),
                if record
                    .flags
                    .contains_all(StartingZooSpawnEntityFlags::VISIBLE)
                {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            if let Some(parent) = parent {
                entity.insert(ChildOf(parent));
            }
            let entity = entity.id();
            super::named_physical_presentation_projection::spawn_required_named_physical_presentations(
                &mut commands, entity, named_presentations,
            );
            pending.record_world_prefab_spawn_completion(entity);
            continue;
        }
        if pending
            .retained_pending_prefab()
            .is_none_or(|(pending_id, _)| *pending_id != prefab_id)
        {
            let Some(handle) = start_asset
                .scene(prefab_id)
                .cloned()
                .or_else(|| active_catalogue.scene(prefab_id))
            else {
                break;
            };
            pending.retain_pending_prefab(prefab_id, handle);
        }
        let prefab_handle = pending
            .retained_pending_prefab()
            .expect("selected prefab handle was retained")
            .1
            .clone();
        let Some(prefab) = prefabs.get(&prefab_handle) else {
            if matches!(
                asset_server.load_state(prefab_handle.id()),
                LoadState::Failed(_)
            ) {
                pending.record_failure(WorldLoadFailure::MissingDependency(prefab_id));
            }
            // Otherwise Bevy is still preparing this selected prefab and its
            // focused dependency handles. Continue next frame.
            break;
        };
        if let Some(model) = first_missing_prefab_collider_model_asset_id(prefab, &models) {
            if prefab
                .loaded_model_asset_handle(model)
                .is_none_or(|handle| {
                    matches!(asset_server.load_state(handle.id()), LoadState::Failed(_))
                })
            {
                pending.record_failure(WorldLoadFailure::MissingDependency(model));
            }
            break;
        }
        let entity = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            prefab,
            prefab_handle,
            root,
            definition_id,
            PersistentId(record.persistent_id),
            transform_from_world(&record.transform),
            record
                .flags
                .contains_all(StartingZooSpawnEntityFlags::VISIBLE),
            parent,
            object_definition_id
                .and_then(|object| active_catalogue.find_object(object))
                .map_or(RigidBody::Static, |object| {
                    if matches!(
                        object.kind,
                        WorldObjectKind::Staff
                            | WorldObjectKind::Guest
                            | WorldObjectKind::Animal
                            | WorldObjectKind::Vehicle
                    ) || active_catalogue
                        .find_placeable(object.id)
                        .is_some_and(|placeable| placeable.moving_footprint)
                    {
                        RigidBody::Dynamic
                    } else {
                        RigidBody::Static
                    }
                }),
        );
        if active_catalogue
            .find_object(definition_id)
            .is_some_and(|definition| definition.terrain_fitted)
        {
            commands.entity(entity).insert(TerrainFittingSurface {
                normal_tolerance: 0.0,
            });
        }
        if is_zoo_entrance(active_catalogue, definition_id) {
            let root_transform =
                transform_from_world(&record.transform).mul_transform(transform_from_authored(
                    &prefab.canonical_scene_prefab_document().entities[0].transform,
                ));
            commands
                .entity(entity)
                .insert(zoo_entrance_from_authored_gate_anchor(
                    root_transform.translation,
                ));
        }
        pending.record_world_prefab_spawn_completion(entity);
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
