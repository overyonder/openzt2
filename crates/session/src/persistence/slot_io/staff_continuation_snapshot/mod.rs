use bevy::{
    ecs::{relationship::RelationshipTarget, resource::IsResource, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex,
    persistent_id_types::PersistentId,
    world_membership_types::{WorldMember, WorldMembers},
};
use openzt2_zoo_staff::staff::{
    staff_cognitive_state::{FossilStaffSensor, StaffNeedAdjustmentTimer},
    staff_job_claiming::StaffRequestSearch,
};
use openzt2_zoo_state::staff::{
    staff_assignment_types::{DisabledTrainerTricks, StaffAssignmentRecord, StaffAssignmentTarget},
    staff_employment_types::Staff,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    restored_component_values::restore_optional_component,
};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct SavedStaffContinuation {
    pub(super) id: u64,
    assignments: Vec<SavedStaffAssignment>,
    disabled_tricks: Vec<AssetId>,
    need_remaining_ns: Option<u64>,
    search: Option<[f64; 2]>,
    fossil: Option<SavedFossilSensor>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
enum SavedStaffAssignment {
    Point(Vec3),
    Animal(u64),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct SavedFossilSensor {
    bad_sites: Vec<u64>,
    last_scan: Option<f64>,
    countdown: f64,
    teleport: bool,
}

impl SavedStaffContinuation {
    pub(super) fn valid(&self, next_id: u64) -> bool {
        let valid_id = |id| id != 0 && id < next_id;
        valid_id(self.id)
            && self.assignments.iter().all(|assignment| match assignment {
                SavedStaffAssignment::Point(point) => point.is_finite(),
                SavedStaffAssignment::Animal(id) => *id != self.id && valid_id(*id),
            })
            && self
                .disabled_tricks
                .iter()
                .all(|id| *id != AssetId::default())
            && self
                .disabled_tricks
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && self
                .search
                .is_none_or(|times| times.into_iter().all(f64::is_finite))
            && self.fossil.as_ref().is_none_or(|sensor| {
                sensor.last_scan.is_none_or(f64::is_finite)
                    && sensor.countdown.is_finite()
                    && sensor.bad_sites.iter().copied().all(valid_id)
            })
    }
}

#[derive(SystemParam)]
pub(super) struct CaptureStaffContinuation<'w, 's> {
    workers: Query<'w, 's, EntityRef<'static>, (With<Staff>, Without<IsResource>)>,
    assignments: Query<
        'w,
        's,
        (
            &'static StaffAssignmentRecord,
            Option<&'static StaffAssignmentTarget>,
        ),
    >,
    identities: Query<'w, 's, (&'static WorldMember, &'static PersistentId)>,
}

impl CaptureStaffContinuation<'_, '_> {
    pub(super) fn capture(
        &self,
        root: Entity,
        members: Option<&WorldMembers>,
    ) -> Result<Vec<SavedStaffContinuation>, Failure> {
        let reference = |entity| {
            let (member, id) = self
                .identities
                .get(entity)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            (member.root == root)
                .then_some(id.0)
                .ok_or(Failure::BrokenPersistentEntityReference)
        };
        let mut records = Vec::new();
        for worker in self
            .workers
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
        {
            let mut assignments = Vec::new();
            for row in worker
                .get::<Children>()
                .into_iter()
                .flat_map(RelationshipTarget::iter)
            {
                let Ok((kind, target)) = self.assignments.get(row) else {
                    continue;
                };
                assignments.push(match kind {
                    StaffAssignmentRecord::Point(point) => SavedStaffAssignment::Point(*point),
                    StaffAssignmentRecord::Animal => {
                        let target = target.ok_or(Failure::SnapshotTransactionInFlight)?;
                        SavedStaffAssignment::Animal(reference(target.0)?)
                    }
                });
            }
            let fossil = worker.get::<FossilStaffSensor>().map(|sensor| {
                let (bad_sites, last_scan, countdown, teleport) = sensor.saved_state();
                SavedFossilSensor {
                    bad_sites: bad_sites
                        .iter()
                        .filter_map(|&site| reference(site).ok())
                        .collect(),
                    last_scan,
                    countdown,
                    teleport,
                }
            });
            records.push(SavedStaffContinuation {
                id: reference(worker.id())?,
                assignments,
                disabled_tricks: worker
                    .get::<DisabledTrainerTricks>()
                    .into_iter()
                    .flat_map(DisabledTrainerTricks::iter)
                    .collect(),
                need_remaining_ns: worker
                    .get::<StaffNeedAdjustmentTimer>()
                    .map(|timer| {
                        u64::try_from(timer.remaining().as_nanos())
                            .map_err(|_| Failure::CapacityExceeded)
                    })
                    .transpose()?,
                search: worker
                    .get::<StaffRequestSearch>()
                    .map(StaffRequestSearch::saved_state),
                fossil,
            });
        }
        records.sort_unstable_by_key(|worker| worker.id);
        Ok(records)
    }
}

#[derive(SystemParam)]
pub(super) struct RestoreStaffContinuation<'w, 's> {
    workers: Query<
        'w,
        's,
        (
            Option<&'static Children>,
            Option<&'static mut DisabledTrainerTricks>,
            Option<&'static mut StaffNeedAdjustmentTimer>,
            Option<&'static mut StaffRequestSearch>,
            Option<&'static mut FossilStaffSensor>,
        ),
        With<Staff>,
    >,
    assignments: Query<'w, 's, (), With<StaffAssignmentRecord>>,
    animals: Query<'w, 's, (), With<openzt2_zoo_state::animal_lifecycle::types::Animal>>,
    identities: Res<'w, PersistentIdIndex>,
}

impl RestoreStaffContinuation<'_, '_> {
    pub(super) fn apply(
        &mut self,
        commands: &mut Commands,
        root: Entity,
        records: &[SavedStaffContinuation],
    ) -> Result<(), Failure> {
        let resolve = |id| {
            self.identities
                .get(root, PersistentId(id))
                .ok_or(Failure::BrokenPersistentEntityReference)
        };
        for saved in records {
            let worker = resolve(saved.id)?;
            let (children, disabled, cadence, search, fossil) = self
                .workers
                .get_mut(worker)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            for row in children.into_iter().flat_map(RelationshipTarget::iter) {
                if self.assignments.contains(row) {
                    commands.entity(row).try_despawn();
                }
            }
            for assignment in &saved.assignments {
                match *assignment {
                    SavedStaffAssignment::Point(point) => {
                        commands.spawn((ChildOf(worker), StaffAssignmentRecord::Point(point)));
                    }
                    SavedStaffAssignment::Animal(id) => {
                        let animal = resolve(id)?;
                        if !self.animals.contains(animal) {
                            return Err(Failure::BrokenPersistentEntityReference);
                        }
                        commands.spawn((
                            ChildOf(worker),
                            StaffAssignmentRecord::Animal,
                            StaffAssignmentTarget(animal),
                        ));
                    }
                }
            }
            let mut worker = commands.entity(worker);
            restore_optional_component(
                &mut worker,
                disabled,
                Some(saved.disabled_tricks.iter().copied().collect()),
                false,
            );
            restore_optional_component(
                &mut worker,
                cadence,
                saved
                    .need_remaining_ns
                    .map(std::time::Duration::from_nanos)
                    .map(StaffNeedAdjustmentTimer::from_remaining),
                false,
            );
            restore_optional_component(
                &mut worker,
                search,
                saved.search.map(StaffRequestSearch::from_saved_state),
                false,
            );
            let sensor = saved
                .fossil
                .as_ref()
                .map(|sensor| {
                    let bad_sites = sensor
                        .bad_sites
                        .iter()
                        .copied()
                        .map(resolve)
                        .collect::<Result<Vec<_>, Failure>>()?;
                    Ok::<_, Failure>(FossilStaffSensor::from_saved_state(
                        bad_sites,
                        sensor.last_scan,
                        sensor.countdown,
                        sensor.teleport,
                    ))
                })
                .transpose()?;
            restore_optional_component(&mut worker, fossil, sensor, false);
        }
        Ok(())
    }
}
