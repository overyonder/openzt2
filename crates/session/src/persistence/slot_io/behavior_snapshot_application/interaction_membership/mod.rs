use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy;

use super::{
    super::behavior_snapshot_types::{BehaviorActorSnapshot, BehaviorSnapshotEntityReference},
    parameters::BehaviorSnapshotApplication,
};
use crate::persistence::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;

pub(super) fn validate_authored_behavior_memberships(
    records: &[BehaviorActorSnapshot],
    application: &BehaviorSnapshotApplication,
) -> Result<bool, Failure> {
    if records.iter().all(|record| record.membership.is_none()) {
        return Ok(true);
    }
    let Some(definitions) = application.definitions.get(&application.definition_assets) else {
        return Ok(false);
    };
    let mut occupied = BTreeMap::new();
    let mut admission_orders = BTreeSet::new();
    for membership in records
        .iter()
        .filter_map(|record| record.membership.as_ref())
    {
        let slot = definitions
            .find_object(membership.definition)
            .and_then(|object| object.interaction_slots.get(membership.slot))
            .ok_or(Failure::UnknownAssetIdentifier)?;
        if membership.docked && slot.is_queue
            || !admission_orders.insert((&membership.container, membership.admission_order))
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        let count = occupied
            .entry((&membership.container, membership.slot))
            .or_insert(0usize);
        *count += 1;
        if *count > usize::from(slot.capacity) {
            return Err(Failure::CapacityExceeded);
        }
    }
    Ok(true)
}

pub(super) fn prepare_behavior_membership_restoration(
    records: &[BehaviorActorSnapshot],
    identities: &BTreeMap<BehaviorSnapshotEntityReference, Entity>,
    application: &BehaviorSnapshotApplication,
) -> Result<InteractionContainerOccupancy, Failure> {
    let mut restored = (*application.occupancy).clone();
    for record in records {
        restored.remove_actor(
            identities[&BehaviorSnapshotEntityReference::Persistent(record.persistent_identifier)],
        );
    }
    let mut memberships = records
        .iter()
        .filter_map(|record| {
            record
                .membership
                .as_ref()
                .map(|membership| (record.persistent_identifier, membership))
        })
        .collect::<Vec<_>>();
    memberships.sort_unstable_by(|(_, left), (_, right)| {
        left.container
            .cmp(&right.container)
            .then_with(|| left.admission_order.cmp(&right.admission_order))
    });
    for (actor, membership) in memberships {
        let actor = identities[&BehaviorSnapshotEntityReference::Persistent(actor)];
        let target = *identities
            .get(&membership.container)
            .ok_or(Failure::BrokenPersistentEntityReference)?;
        if !application.entities.get(target).is_ok_and(|target| {
            target
                .definition_id
                .is_some_and(|definition| definition.0 == membership.definition)
        }) {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        let capacity = application
            .definitions
            .get(&application.definition_assets)
            .and_then(|definitions| definitions.find_object(membership.definition))
            .and_then(|object| object.interaction_slots.get(membership.slot))
            .map(|slot| slot.capacity)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        if !restored.admit(target, actor, membership.slot, capacity) {
            return Err(Failure::CapacityExceeded);
        }
        if membership.docked {
            restored.mark_docked(target, actor);
        }
    }
    Ok(restored)
}
