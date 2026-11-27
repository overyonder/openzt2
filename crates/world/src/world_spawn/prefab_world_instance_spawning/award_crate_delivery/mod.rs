use avian3d::prelude::{RigidBody, RigidBodyDisabled};
use bevy::{asset::AssetLoadFailedEvent, prelude::*};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_entity_crating::AnimalCrate,
    world_membership_types::WorldMember,
    zoo_entrance_anchor_synchronization::ZooEntrance,
};

use super::spawn_loaded_scene_prefab_as_world_instance;

#[derive(Message)]
pub struct PlaceAwardCratedObject {
    pub root: Entity,
    pub definition: AssetId,
}

#[derive(Message)]
pub struct DeliverCratedAwardAnimal {
    pub crate_entity: Entity,
    pub member: WorldMember,
    pub definition: AssetId,
    pub persistent_id: PersistentId,
    pub pose: Transform,
}

#[derive(Component)]
struct PendingAwardCrate {
    definition: AssetId,
    crate_type: AssetId,
    container: Handle<ScenePrefabAsset>,
    object: Option<Handle<ScenePrefabAsset>>,
}

#[derive(Component)]
pub(in crate::world_spawn) struct AwardCrateObjectOccupant;

pub(super) fn register_award_crate_delivery(app: &mut App) {
    app.add_message::<PlaceAwardCratedObject>()
        .add_message::<DeliverCratedAwardAnimal>()
        .add_systems(
            Update,
            (
                demand_award_delivery_prefabs.run_if(on_message::<PlaceAwardCratedObject>),
                place_ready_award_crates
                    .after(demand_award_delivery_prefabs)
                    .run_if(
                        any_match_filter::<Added<PendingAwardCrate>>
                            .or_eager(on_message::<AssetEvent<ScenePrefabAsset>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<ScenePrefabAsset>>),
                    ),
            ),
        );
}

fn demand_award_delivery_prefabs(
    mut commands: Commands,
    mut requests: MessageReader<PlaceAwardCratedObject>,
    catalogue: Res<WorldDefinitions>,
    documents: Res<Assets<WorldDefinitionAsset>>,
    species: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
) {
    let definitions = catalogue.get(&documents);
    let species = species.get(&species_assets);
    for request in requests.read() {
        let Some(definitions) = definitions else {
            continue;
        };
        let animal = species.is_some_and(|species| {
            species.find_variant(request.definition).is_some()
                || species.find(request.definition).is_some()
        });
        let crate_type = if animal {
            AssetId::from_key("animalcrate_df")
        } else {
            AssetId::from_key("objectcrate_df")
        };
        let Some(container) = definitions
            .find_object(crate_type)
            .and_then(|row| definitions.scene(row.prefab))
        else {
            warn!(?crate_type, "award crate has no placeable prefab");
            continue;
        };
        let object = if animal {
            None
        } else {
            let Some(handle) = definitions
                .find_object(request.definition)
                .and_then(|row| definitions.scene(row.prefab))
            else {
                warn!(definition = ?request.definition, "award object has no placeable prefab");
                continue;
            };
            Some(handle)
        };
        commands.spawn((
            WorldMember { root: request.root },
            PendingAwardCrate {
                definition: request.definition,
                crate_type,
                container,
                object,
            },
        ));
    }
}

fn place_ready_award_crates(
    mut commands: Commands,
    server: Res<AssetServer>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    requests: Query<(Entity, &WorldMember, &PendingAwardCrate)>,
    entrances: Query<(&WorldMember, &ZooEntrance)>,
    mut allocators: Query<&mut PersistentIdAllocator>,
    mut animals: MessageWriter<DeliverCratedAwardAnimal>,
) {
    for (operation, member, request) in &requests {
        if std::iter::once(&request.container)
            .chain(request.object.iter())
            .any(|handle| {
                matches!(
                    server.load_state(handle.id()),
                    bevy::asset::LoadState::Failed(_)
                )
            })
        {
            warn!(definition = ?request.definition, "award delivery prefab failed to load");
            commands.entity(operation).try_despawn();
            continue;
        }
        let Some(container) = prefabs.get(&request.container) else {
            continue;
        };
        let object = request
            .object
            .as_ref()
            .and_then(|handle| prefabs.get(handle).map(|prefab| (handle, prefab)));
        if request.object.is_some() && object.is_none() {
            continue;
        }
        let Ok(mut allocator) = allocators.get_mut(member.root) else {
            commands.entity(operation).try_despawn();
            continue;
        };
        let Some((_, gate)) = entrances.iter().find(|(gate, _)| gate.root == member.root) else {
            warn!(world = ?member.root, "award delivery requires a zoo entrance");
            commands.entity(operation).try_despawn();
            continue;
        };
        if allocator.next().checked_add(2).is_none() {
            commands.entity(operation).try_despawn();
            continue;
        }
        let crate_id = allocator
            .allocate()
            .expect("award requires two reserved IDs");
        let persistent_id = allocator
            .allocate()
            .expect("award requires two reserved IDs");
        let pose = Transform::from_translation(gate.arrival);
        let container = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            container,
            request.container.clone(),
            member.root,
            request.crate_type,
            crate_id,
            pose,
            true,
            None,
            RigidBody::Static,
        );
        if let Some((handle, object)) = object {
            let content = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                object,
                handle.clone(),
                member.root,
                request.definition,
                persistent_id,
                pose,
                false,
                None,
                RigidBody::Static,
            );
            commands.entity(content).insert((
                AnimalCrate(container),
                AwardCrateObjectOccupant,
                RigidBodyDisabled,
            ));
        } else {
            animals.write(DeliverCratedAwardAnimal {
                crate_entity: container,
                member: *member,
                definition: request.definition,
                persistent_id,
                pose,
            });
        }
        commands.entity(operation).try_despawn();
    }
}
