use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;

use super::types::{Dead, Rampaging};

/// Asks the recovery team to tranquilize a newly rampaging animal. Disease
/// and escape requests come from the animal's own authored staff-request
/// controllers instead.
pub(super) fn request_tranquilization_for_rampaging_animals(
    rampaging_animals: Query<Entity, (With<Animal>, Added<Rampaging>, Without<Dead>)>,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    for animal in &rampaging_animals {
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Tranquilize,
            target: animal,
            urgency: openzt2_zoo_state::staff::staff_job_types::authored_staff_request_urgency(
                openzt2_game_data::world_definitions::staff_management::StaffRequestData::CONSTRUCTED_DEFAULT.priority,
            ).expect("the default staff priority is positive"),
            token: Some(AssetId::from_key("t_tranqdino")),
        });
    }
}
