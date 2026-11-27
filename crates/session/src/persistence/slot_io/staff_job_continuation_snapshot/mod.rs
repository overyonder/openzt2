use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex,
    persistent_id_types::PersistentId,
    world_membership_types::{WorldMember, WorldMembers},
};
use openzt2_zoo_state::staff::{
    staff_employment_types::Staff,
    staff_job_types::{
        JobClaim, StaffJob, StaffJobCancellationPending, StaffJobRouting, StaffJobTarget,
        TankCleaningRequestSite, UnsuccessfulStaffJobCandidates,
    },
};
use serde::{Deserialize, Serialize};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SavedStaffJob {
    pub id: u64,
    pub target: Option<SavedStaffJobTarget>,
    kind: Option<StaffJobKind>,
    point: Option<[f32; 3]>,
    priority: f32,
    issued_at_seconds: f32,
    remaining_wait_seconds: f32,
    first_attempt: bool,
    token: Option<AssetId>,
    subject_type: Option<AssetId>,
    recipient: Option<AssetId>,
    pub rejected: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(super) enum SavedStaffJobTarget {
    Entity(u64),
    CleaningSite {
        id: u64,
        tank: u64,
        position: [f32; 3],
    },
}

impl SavedStaffJobTarget {
    pub(super) fn existing_id(self) -> u64 {
        match self {
            Self::Entity(id) => id,
            Self::CleaningSite { tank, .. } => tank,
        }
    }
}

impl SavedStaffJob {
    pub(super) fn valid(&self, next: u64) -> bool {
        let valid_id = |id| id != 0 && id < next;
        valid_id(self.id)
            && self.target.is_none_or(|target| {
                valid_id(target.existing_id())
                    && match target {
                        SavedStaffJobTarget::Entity(_) => true,
                        SavedStaffJobTarget::CleaningSite { id, position, .. } => {
                            valid_id(id) && position.iter().all(|axis| axis.is_finite())
                        }
                    }
            })
            && (self.point.is_some() || self.target.is_some())
            && self
                .point
                .is_none_or(|point| point.iter().all(|value| value.is_finite()))
            && self.priority.is_finite()
            && self.issued_at_seconds.is_finite()
            && self.remaining_wait_seconds.is_finite()
            && self.rejected.iter().copied().all(valid_id)
            && self.rejected.windows(2).all(|pair| pair[0] < pair[1])
    }

    pub(super) fn restore(
        &self,
        commands: &mut Commands,
        root: Entity,
        identities: &PersistentIdIndex,
        sites: &mut std::collections::BTreeMap<u64, Entity>,
    ) -> Result<(), Failure> {
        let resolve = |id| {
            identities
                .get(root, PersistentId(id))
                .ok_or(Failure::BrokenPersistentEntityReference)
        };
        let target = match self.target {
            None => None,
            Some(SavedStaffJobTarget::Entity(id)) => Some(resolve(id)?),
            Some(SavedStaffJobTarget::CleaningSite { id, tank, position }) => {
                let tank = resolve(tank)?;
                Some(*sites.entry(id).or_insert_with(|| {
                    commands
                        .spawn((
                            PersistentId(id),
                            WorldMember { root },
                            TankCleaningRequestSite { tank },
                            Transform::from_translation(Vec3::from_array(position)),
                        ))
                        .id()
                }))
            }
        };
        let mut candidates = UnsuccessfulStaffJobCandidates::default();
        for &id in &self.rejected {
            candidates.record(resolve(id)?);
        }
        let mut job = commands.spawn((
            PersistentId(self.id),
            WorldMember { root },
            StaffJob {
                kind: self.kind,
                target,
                point: self.point.map(Vec3::from_array),
                priority: self.priority,
                issued_at_seconds: self.issued_at_seconds,
                remaining_wait_seconds: self.remaining_wait_seconds,
                first_attempt: self.first_attempt,
                token: self.token,
            },
            StaffJobRouting {
                subject_type: self.subject_type,
                recipient: self.recipient,
            },
        ));
        if let Some(target) = target {
            job.insert(StaffJobTarget(target));
        }
        if !self.rejected.is_empty() {
            job.insert(candidates);
        }
        Ok(())
    }
}

#[derive(SystemParam)]
pub(super) struct CaptureStaffJobs<'w, 's> {
    jobs: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static StaffJob,
            Option<&'static StaffJobTarget>,
            Option<&'static StaffJobRouting>,
            Option<&'static UnsuccessfulStaffJobCandidates>,
        ),
        (Without<JobClaim>, Without<StaffJobCancellationPending>),
    >,
    staff: Query<'w, 's, (Entity, &'static PersistentId), With<Staff>>,
    ids: Query<'w, 's, (&'static PersistentId, &'static WorldMember)>,
    sites: Query<'w, 's, (&'static TankCleaningRequestSite, &'static Transform)>,
}

impl CaptureStaffJobs<'_, '_> {
    pub(super) fn capture(
        &self,
        root: Entity,
        members: Option<&WorldMembers>,
    ) -> Result<Vec<SavedStaffJob>, Failure> {
        let mut records = Vec::new();
        let mut staff = None;
        for (id, job, target, routing, rejected) in self
            .jobs
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .matched()
        {
            let scoped_id = |entity| {
                self.ids
                    .get(entity)
                    .ok()
                    .filter(|(_, member)| member.root == root)
                    .map(|(id, _)| id.0)
                    .ok_or(Failure::BrokenPersistentEntityReference)
            };
            let target = target
                .map(|target| -> Result<_, Failure> {
                    let id = scoped_id(target.0)?;
                    if let Ok((site, pose)) = self.sites.get(target.0) {
                        Ok(SavedStaffJobTarget::CleaningSite {
                            id,
                            tank: scoped_id(site.tank)?,
                            position: pose.translation.to_array(),
                        })
                    } else {
                        Ok(SavedStaffJobTarget::Entity(id))
                    }
                })
                .transpose()?;
            if target.is_none() && job.point.is_none() {
                continue;
            }
            let mut rejected = rejected.map_or_else(Vec::new, |rejected| {
                staff
                    .get_or_insert_with(|| {
                        self.staff
                            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
                            .matched()
                            .collect::<Vec<_>>()
                    })
                    .iter()
                    .filter_map(|(entity, id)| rejected.contains(*entity).then_some(id.0))
                    .collect()
            });
            rejected.sort_unstable();
            let routing = routing.copied().unwrap_or_default();
            records.push(SavedStaffJob {
                id: id.0,
                target,
                kind: job.kind,
                point: job.point.map(|point| point.to_array()),
                priority: job.priority,
                issued_at_seconds: job.issued_at_seconds,
                remaining_wait_seconds: job.remaining_wait_seconds,
                first_attempt: job.first_attempt,
                token: job.token,
                subject_type: routing.subject_type,
                recipient: routing.recipient,
                rejected,
            });
        }
        records.sort_unstable_by_key(|job| job.id);
        Ok(records)
    }
}
