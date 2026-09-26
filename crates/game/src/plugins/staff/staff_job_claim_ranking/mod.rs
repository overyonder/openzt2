use bevy::prelude::*;

pub(crate) const MAXIMUM_RANKED_STAFF_JOB_CANDIDATES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RankedStaffJobClaimCandidate {
    pub job: Entity,
    pub staff: Entity,
    pub urgency: u16,
    pub authored_priority: f32,
    pub distance_squared: f32,
}

impl RankedStaffJobClaimCandidate {
    pub(crate) fn outranks(self, other: Self) -> bool {
        self.urgency > other.urgency
            || (self.urgency == other.urgency
                && (self.authored_priority > other.authored_priority
                    || (self.authored_priority == other.authored_priority
                        && (self.distance_squared < other.distance_squared
                            || (self.distance_squared == other.distance_squared
                                && staff_job_and_worker_entity_keys(self.job, self.staff)
                                    < staff_job_and_worker_entity_keys(other.job, other.staff))))))
    }
}

fn staff_job_and_worker_entity_keys(job: Entity, staff: Entity) -> (u64, u64) {
    (job.to_bits(), staff.to_bits())
}

pub(crate) fn insert_candidate_into_ranked_staff_job_claim_candidates(
    candidates: &mut [Option<RankedStaffJobClaimCandidate>; MAXIMUM_RANKED_STAFF_JOB_CANDIDATES],
    candidate: RankedStaffJobClaimCandidate,
) {
    let mut insertion_index = candidates
        .iter()
        .position(|slot| slot.is_none_or(|current| candidate.outranks(current)))
        .unwrap_or(MAXIMUM_RANKED_STAFF_JOB_CANDIDATES);
    if insertion_index == MAXIMUM_RANKED_STAFF_JOB_CANDIDATES {
        return;
    }
    let mut displaced_candidate = Some(candidate);
    while insertion_index < MAXIMUM_RANKED_STAFF_JOB_CANDIDATES {
        std::mem::swap(&mut candidates[insertion_index], &mut displaced_candidate);
        if displaced_candidate.is_none() {
            break;
        }
        insertion_index += 1;
    }
}
