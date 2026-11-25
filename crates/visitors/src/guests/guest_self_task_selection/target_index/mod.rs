use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityRequirements, BehaviorCandidateRole,
        BehaviorEligibilityFactJunction,
    },
    world_definitions::world_objects::WorldObjectDefinition,
    AssetId,
};

pub(super) struct GuestTarget<'a> {
    pub(super) entity: Entity,
    pub(super) object: &'a WorldObjectDefinition,
    pub(super) position: Vec3,
    pub(super) waste: Option<u16>,
}

#[derive(Default)]
pub(super) struct GuestTargetIndex<'a> {
    targets: Vec<GuestTarget<'a>>,
    by_world_and_type: HashMap<(Entity, Option<AssetId>), Vec<usize>>,
}

impl<'a> GuestTargetIndex<'a> {
    pub(super) fn insert(&mut self, world: Entity, target: GuestTarget<'a>) {
        let ordinal = self.targets.len();
        self.by_world_and_type
            .entry((world, None))
            .or_default()
            .push(ordinal);
        for candidate_type in
            std::iter::once(&target.object.id).chain(&target.object.behavior_type_identifiers)
        {
            let bucket = self
                .by_world_and_type
                .entry((world, Some(*candidate_type)))
                .or_default();
            if bucket.last() != Some(&ordinal) {
                bucket.push(ordinal);
            }
        }
        self.targets.push(target);
    }

    pub(super) fn candidates<'s>(
        &'s self,
        world: Entity,
        task_targets: &[String],
        requirements: &[BehaviorCandidateEligibilityRequirements],
        combined: &'s mut Vec<usize>,
    ) -> impl Iterator<Item = &'s GuestTarget<'a>> {
        let bucket = |candidate_type| {
            self.by_world_and_type
                .get(&(world, candidate_type))
                .map_or(&[][..], Vec::as_slice)
        };
        // Each requirement must pass. A single typed one gives us a candidate
        // subset, and the caller checks all remaining types and live facts.
        let typed_requirement = requirements.iter().find(|requirement| {
            requirement.candidate_role == BehaviorCandidateRole::Target
                && !requirement.candidate_type_identifiers.is_empty()
        });
        let ordinals = match typed_requirement {
            None if task_targets.is_empty() => bucket(None),
            None => {
                combined.clear();
                for target in task_targets
                    .iter()
                    .map(String::as_str)
                    .filter(|target| !matches!(*target, "self" | "path" | "fromtoken"))
                {
                    combined.extend_from_slice(bucket(Some(AssetId::from_key(target))));
                }
                combined.sort_unstable();
                combined.dedup();
                combined.as_slice()
            }
            Some(requirement) => match requirement.candidate_type_junction {
                BehaviorEligibilityFactJunction::All => {
                    bucket(requirement.candidate_type_identifiers.first().copied())
                }
                BehaviorEligibilityFactJunction::Any => {
                    combined.clear();
                    for &candidate_type in &requirement.candidate_type_identifiers {
                        combined.extend_from_slice(bucket(Some(candidate_type)));
                    }
                    // Preserve query order so equal scores consume RNG in the same order.
                    combined.sort_unstable();
                    combined.dedup();
                    combined.as_slice()
                }
            },
        };
        ordinals.iter().map(|&ordinal| &self.targets[ordinal])
    }
}
