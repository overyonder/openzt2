use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_data::world_definitions::staff_management::StaffRequestData;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;

use super::types::{Dead, Rampaging};

/// Asks the recovery team to tranquilize a newly rampaging animal. Disease
/// and escape requests come from the animal's own authored staff-request
/// controllers instead.
pub(super) fn request_tranquilization_for_rampaging_animals(
    rampaging_animals: Query<
        (Entity, &WorldMember),
        (With<Animal>, Added<Rampaging>, Without<Dead>),
    >,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    for (animal, member) in &rampaging_animals {
        staff_job_requests.write(StaffJobRequest {
            kind: Some(StaffJobKind::Tranquilize),
            root: member.root,
            target: Some(animal),
            point: None,
            priority: StaffRequestData::CONSTRUCTED_DEFAULT.priority,
            issued_at_seconds: 0.0,
            remaining_wait_seconds: 0.0,
            token: Some(AssetId::from_key("t_tranqdino")),
            routing: Default::default(),
        });
    }
}
