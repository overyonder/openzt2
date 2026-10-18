use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobRequest};

use super::types::{Dead, Disease, Escaped, Rampaging};

/// Turns newly authoritative animal-health facts into ordinary staff work.
pub(super) fn request_staff_jobs_for_diseased_rampaging_escaped_and_tranquilized_animals(
    diseased_animals: Query<(Entity, &Disease), (With<Animal>, Added<Disease>, Without<Dead>)>,
    rampaging_animals: Query<Entity, (With<Animal>, Added<Rampaging>, Without<Dead>)>,
    escaped_animals: Query<
        Entity,
        (
            With<Animal>,
            Added<Escaped>,
            Without<Rampaging>,
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
    // Recovery teams tranquilize rampaging dinosaurs.
    for animal in &rampaging_animals {
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Tranquilize,
            target: animal,
            urgency: 0,
            token: Some(AssetId::from_key("t_tranqdino")),
        });
    }
    // `animal.xml` asks a keeper to crate an escaped animal (`b_Escaped`).
    for animal in &escaped_animals {
        staff_job_requests.write(StaffJobRequest {
            kind: StaffJobKind::Capture,
            target: animal,
            urgency: 0,
            token: Some(AssetId::from_key("t_crateanimal")),
        });
    }
}

/// A keeper no longer needs to crate an animal that returned to, or was put
/// back into, containment.
pub(super) fn cancel_capture_jobs_for_animals_no_longer_escaped(
    mut no_longer_escaped: RemovedComponents<Escaped>,
    jobs: Query<(Entity, &StaffJob)>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for animal in no_longer_escaped.read() {
        for (job, _) in jobs
            .iter()
            .filter(|(_, job)| job.kind == StaffJobKind::Capture && job.target == animal)
        {
            cancellations.write(CancelStaffJobRequest { job });
        }
    }
}
