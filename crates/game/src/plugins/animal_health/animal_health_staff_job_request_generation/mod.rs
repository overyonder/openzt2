use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use crate::plugins::{
    animal_lifecycle::types::Animal, staff::staff_lifecycle_messages::StaffJobRequest,
};

use super::types::{Dead, Disease, Escaped, Rampaging, Tranquilized};

/// Turns newly authoritative animal-health facts into ordinary staff work.
pub(super) fn request_staff_jobs_for_diseased_rampaging_escaped_and_tranquilized_animals(
    diseased_animals: Query<(Entity, &Disease), (With<Animal>, Added<Disease>, Without<Dead>)>,
    rampaging_animals: Query<Entity, (With<Animal>, Added<Rampaging>, Without<Dead>)>,
    escaped_animals: Query<Entity, (With<Animal>, Added<Escaped>, Without<Dead>)>,
    tranquilized_escaped_animals: Query<
        Entity,
        (
            With<Animal>,
            With<Escaped>,
            Added<Tranquilized>,
            Without<Dead>,
        ),
    >,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    for (animal, disease) in &diseased_animals {
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Treat,
            target: animal,
            urgency: disease.severity_permille,
            token: Some(AssetId::from_key("t_cureanimal")),
        });
    }
    for animal in rampaging_animals.iter().chain(&escaped_animals) {
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Tranquilize,
            target: animal,
            urgency: 0,
            token: Some(AssetId::from_key("t_tranqdino")),
        });
    }
    for animal in &tranquilized_escaped_animals {
        // Capture token selection for keepers and recovery teams is not implemented.
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Capture,
            target: animal,
            urgency: 0,
            token: None,
        });
    }
}
