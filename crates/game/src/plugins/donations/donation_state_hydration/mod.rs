use bevy::prelude::*;
use openzt2_game_data::{world_definitions::world_objects::WorldObjectPropertyFlags, AssetId};

use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::facility_economy_types::FacilityProfit;
use crate::plugins::economy::facility_economy_types::OperatingSinceDay;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    donation_guest_decision_types::DonationRng,
    donation_opportunity_types::{
        DonationAcceptor, DonationAcceptorFactsResolved, DonationUsePoint,
    },
};

const DONATION_ACCEPTOR_DOCK_ATTACHMENT_NAME: &str = "AdultDock";

/// Projects the source-owned acceptor flag and prefab-owned dock attachment
/// onto each ordinary spawned world-object entity.
pub(super) fn hydrate_authored_donation_acceptors_and_docking_points(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    candidates: Query<
        (Entity, &DefinitionId, &GlobalTransform),
        Without<DonationAcceptorFactsResolved>,
    >,
    attachments: Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        &GlobalTransform,
    )>,
    parents: Query<&ChildOf>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let adult_dock = AssetId::from_key(DONATION_ACCEPTOR_DOCK_ATTACHMENT_NAME);
    for (entity, definition, entity_transform) in &candidates {
        let Some(object) = world_definitions.find_object(definition.0) else {
            commands
                .entity(entity)
                .insert(DonationAcceptorFactsResolved);
            continue;
        };
        if !object
            .properties
            .contains_all(WorldObjectPropertyFlags::DONATION_ACCEPTOR)
        {
            commands
                .entity(entity)
                .insert(DonationAcceptorFactsResolved);
            continue;
        }
        let Some(dock_transform) =
            attachments
                .iter()
                .find_map(|(attachment, identifier, attachment_transform)| {
                    (identifier.0 == adult_dock
                        && entity_is_descendant_of(attachment, entity, &parents))
                    .then_some(attachment_transform)
                })
        else {
            continue;
        };
        let local_dock_transform = dock_transform.reparented_to(entity_transform);
        commands.entity(entity).insert((
            DonationAcceptor { beneficiary: None },
            DonationUsePoint {
                local_point: local_dock_transform.translation,
                local_forward: local_dock_transform.rotation * Vec3::Z,
            },
            DonationAcceptorFactsResolved,
        ));
    }
}

fn entity_is_descendant_of(
    mut entity: Entity,
    ancestor: Entity,
    parents: &Query<&ChildOf>,
) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        let Ok(parent) = parents.get(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

pub(super) fn initialize_new_donation_acceptors_with_operating_age_and_profit(
    mut commands: Commands,
    clock: Res<ZooClock>,
    acceptors: Query<Entity, (With<DonationAcceptor>, Without<OperatingSinceDay>)>,
) {
    for acceptor in &acceptors {
        commands.entity(acceptor).insert((
            OperatingSinceDay(clock.absolute_day),
            FacilityProfit::default(),
        ));
    }
}

pub(super) fn initialize_new_guests_with_deterministic_donation_random_streams(
    mut commands: Commands,
    zoo_seed: Res<ZooSeed>,
    guests: Query<(Entity, &PersistentId), (With<Guest>, Without<DonationRng>)>,
) {
    for (guest, persistent_id) in &guests {
        commands
            .entity(guest)
            .insert(DonationRng(DeterministicRng::from_entity(
                *zoo_seed,
                *persistent_id,
                RngDomain::Donation,
            )));
    }
}
