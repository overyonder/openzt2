use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;

use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::maintenance::maintenance_types::{
    AnimalHabitatWaste, FacilityContainedWaste, LooseLitterWaste,
    MaintainableObjectConditionPermille, ZooCleanlinessPermille,
};
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::{JobClaim, StaffJob};
use openzt2_zoo_state::staff::staff_lifecycle_messages::{StaffJobCancelled, StaffJobCompleted};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Staff work as the player would see it: waste and litter waiting for
/// cleaning, full bins, worn objects, open and claimed jobs per kind, and
/// staff walking somewhere. Completions, cancellations and staff task
/// failures are counted for the whole game process, every frame, because a
/// job can start and finish between journey snapshots; compare them against
/// a snapshot.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
pub(super) fn collect_staff_work_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut completed: MessageReader<StaffJobCompleted>,
    mut cancelled: MessageReader<StaffJobCancelled>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    mut totals: Local<HashMap<String, u32>>,
    staff: Query<Has<Destination>, With<Staff>>,
    jobs: Query<(&StaffJob, Has<JobClaim>)>,
    habitat_waste: Query<&AnimalHabitatWaste>,
    litter: Query<&LooseLitterWaste>,
    bins: Query<&FacilityContainedWaste>,
    conditions: Query<&MaintainableObjectConditionPermille>,
    cleanliness: Option<Res<ZooCleanlinessPermille>>,
) {
    for completion in completed.read() {
        *totals
            .entry(format!(
                "staff_jobs_completed_{}",
                kind_name(completion.kind)
            ))
            .or_default() += 1;
    }
    for cancellation in cancelled.read() {
        *totals
            .entry(format!(
                "staff_jobs_cancelled_{}",
                kind_name(cancellation.kind)
            ))
            .or_default() += 1;
    }
    let staff_failures = failed
        .read()
        .filter(|failure| staff.contains(failure.actor))
        .count();
    *totals.entry("staff_task_failures".to_owned()).or_default() += staff_failures as u32;
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let count = |value: usize| value as f64;
    let mut facts = vec![
        (
            "staff_moving".to_owned(),
            count(staff.iter().filter(|moving| *moving).count()),
        ),
        (
            "animal_habitat_waste".to_owned(),
            count(habitat_waste.iter().count()),
        ),
        ("loose_litter".to_owned(), count(litter.iter().count())),
        (
            "waste_bin_units".to_owned(),
            bins.iter()
                .map(|bin| f64::from(bin.contained_waste_units))
                .sum(),
        ),
        (
            "full_waste_bins".to_owned(),
            count(
                bins.iter()
                    .filter(|bin| bin.contained_waste_units >= bin.contained_waste_capacity_units)
                    .count(),
            ),
        ),
        (
            "worn_objects".to_owned(),
            count(
                conditions
                    .iter()
                    .filter(|condition| condition.0 < 1_000)
                    .count(),
            ),
        ),
        (
            "lowest_object_condition_permille".to_owned(),
            conditions
                .iter()
                .map(|condition| f64::from(condition.0))
                .fold(1_000.0, f64::min),
        ),
        (
            "zoo_cleanliness_permille".to_owned(),
            cleanliness
                .and_then(|cleanliness| cleanliness.0)
                .map_or(-1.0, f64::from),
        ),
    ];
    for kind in ALL_KINDS {
        let (open, claimed) = jobs.iter().filter(|(job, _)| job.kind == kind).fold(
            (0, 0),
            |(open, claimed), (_, is_claimed)| {
                (
                    open + usize::from(!is_claimed),
                    claimed + usize::from(is_claimed),
                )
            },
        );
        facts.push((format!("staff_jobs_open_{}", kind_name(kind)), count(open)));
        facts.push((
            format!("staff_jobs_claimed_{}", kind_name(kind)),
            count(claimed),
        ));
        for total in ["completed", "cancelled"] {
            let name = format!("staff_jobs_{total}_{}", kind_name(kind));
            let value = totals.get(&name).copied().unwrap_or_default();
            facts.push((name, f64::from(value)));
        }
    }
    // Guests drop trash in a bin or on the ground, and either job clears it.
    let fact = |facts: &[(String, f64)], name: &str| {
        facts
            .iter()
            .find(|(fact, _)| fact == name)
            .map_or(0.0, |(_, value)| *value)
    };
    let trash_waiting = fact(&facts, "loose_litter") + fact(&facts, "waste_bin_units");
    let trash_jobs_completed = fact(&facts, "staff_jobs_completed_sweep_litter")
        + fact(&facts, "staff_jobs_completed_empty_bin");
    facts.push(("trash_waiting".to_owned(), trash_waiting));
    facts.push(("trash_jobs_completed".to_owned(), trash_jobs_completed));
    facts.push((
        "staff_task_failures".to_owned(),
        f64::from(
            totals
                .get("staff_task_failures")
                .copied()
                .unwrap_or_default(),
        ),
    ));
    report.current_facts.extend(facts);
}

const ALL_KINDS: [StaffJobKind; 13] = [
    StaffJobKind::Feed,
    StaffJobKind::RefillWater,
    StaffJobKind::CleanHabitat,
    StaffJobKind::EmptyBin,
    StaffJobKind::SweepLitter,
    StaffJobKind::Repair,
    StaffJobKind::Treat,
    StaffJobKind::Educate,
    StaffJobKind::Entertain,
    StaffJobKind::Tranquilize,
    StaffJobKind::Capture,
    StaffJobKind::MaintainTank,
    StaffJobKind::OperateShow,
];

fn kind_name(kind: StaffJobKind) -> &'static str {
    match kind {
        StaffJobKind::Feed => "feed",
        StaffJobKind::RefillWater => "refill_water",
        StaffJobKind::CleanHabitat => "clean_habitat",
        StaffJobKind::EmptyBin => "empty_bin",
        StaffJobKind::SweepLitter => "sweep_litter",
        StaffJobKind::Repair => "repair",
        StaffJobKind::Treat => "treat",
        StaffJobKind::Educate => "educate",
        StaffJobKind::Entertain => "entertain",
        StaffJobKind::Tranquilize => "tranquilize",
        StaffJobKind::Capture => "capture",
        StaffJobKind::MaintainTank => "maintain_tank",
        StaffJobKind::OperateShow => "operate_show",
    }
}
