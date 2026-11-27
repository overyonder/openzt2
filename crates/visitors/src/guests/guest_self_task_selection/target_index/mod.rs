use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityRequirements, BehaviorCandidateRole,
        BehaviorEligibilityFactJunction,
    },
    AssetId,
};

pub(super) fn candidate_types(
    task_targets: &[String],
    requirements: &[BehaviorCandidateEligibilityRequirements],
    types: &mut Vec<AssetId>,
) {
    types.clear();
    if let Some(requirement) = requirements.iter().find(|requirement| {
        requirement.candidate_role == BehaviorCandidateRole::Target
            && !requirement.candidate_type_identifiers.is_empty()
    }) {
        match requirement.candidate_type_junction {
            BehaviorEligibilityFactJunction::All => {
                types.push(requirement.candidate_type_identifiers[0]);
            }
            BehaviorEligibilityFactJunction::Any => {
                types.extend(requirement.candidate_type_identifiers.iter().copied());
            }
        }
    } else {
        types.extend(
            task_targets
                .iter()
                .map(String::as_str)
                .filter(|target| !matches!(*target, "self" | "path" | "fromtoken"))
                .map(AssetId::from_key),
        );
    }
}
