use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use bevy::prelude::*;
use openzt2_game_data::{
    behavior::{
        document::BehaviorTask,
        eligibility::{
            BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
            BehaviorCandidateRole, BehaviorConditionEligibilityFact,
            BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
            BehaviorEligibilityFactJunction, BehaviorEligibilityFactValue,
            BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
        },
    },
    world_definitions::staff_management::StaffRoleDefinition,
    AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDeclarationIndexView, BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};

use super::staff_assignment_types::{StaffAssignmentArea, StaffAssignmentTarget};

pub(super) struct CompatibleStaffBehaviorTask<'a> {
    pub(super) document: Handle<BehaviorDocumentAsset>,
    pub(super) declaration_index: usize,
    pub(super) definition: &'a BehaviorTask,
}

/// Facts supplied by the staff claim boundary for authored candidate predicates.
/// `None` is deliberate: an unsupported fact rejects the candidate rather than
/// turning an unavailable native predicate into an implicit pass.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct StaffCandidateEligibilityFacts {
    pub(super) target_definition: Option<AssetId>,
    pub(super) target_in_water: Option<bool>,
    pub(super) target_on_land: Option<bool>,
    pub(super) target_rampaging: Option<bool>,
    pub(super) target_in_show: Option<bool>,
    pub(super) target_food_level_q16: Option<i32>,
    pub(super) target_trash_level_q16: Option<i32>,
    pub(super) target_filter_dirtiness_q16: Option<i32>,
    /// The job targets a manager request site, which a task authored to
    /// target its own subject (`self`) moves to, as keeper tank cleaning does.
    pub(super) target_is_request_site: bool,
}

#[derive(Resource, Default)]
pub(super) struct StaffTasksByRequestToken(std::collections::BTreeMap<AssetId, Vec<AssetId>>);

impl StaffTasksByRequestToken {
    pub(super) fn candidates(&self, token: AssetId) -> impl Iterator<Item = AssetId> + '_ {
        self.0.get(&token).into_iter().flatten().copied()
    }
}

pub(super) fn index_winning_staff_token_tasks(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    mut index: ResMut<StaffTasksByRequestToken>,
) {
    if !documents.is_changed() && !assets.is_changed() {
        return;
    }
    let Some(declarations) = documents.create_declaration_index_view(&assets) else {
        index.0.clear();
        return;
    };
    index.0.clear();
    for (_, _, task) in declarations.behavior_tasks() {
        for object in &task.objects {
            let candidates = index
                .0
                .entry(AssetId::from_key(&object.to_ascii_lowercase()))
                .or_default();
            if candidates.last() != Some(&task.id) {
                candidates.push(task.id);
            }
        }
    }
}

pub(super) fn staff_subject_matches(
    definitions: WorldDefinitionsView<'_>,
    role: &StaffRoleDefinition,
    required: AssetId,
) -> bool {
    required == role.id
        || required == role.object
        || required == AssetId::from_key("staff")
        || definitions
            .find_object(role.object)
            .is_some_and(|object| object.behavior_type_identifiers.contains(&required))
}

pub(super) fn find_highest_priority_compatible_staff_behavior_task<'a>(
    behavior_documents: BehaviorDeclarationIndexView<'a>,
    index: &StaffTasksByRequestToken,
    definitions: WorldDefinitionsView<'_>,
    role: &StaffRoleDefinition,
    request_token: AssetId,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> Option<CompatibleStaffBehaviorTask<'a>> {
    let target_definition = candidate_facts
        .target_definition
        .and_then(|id| definitions.find_object(id));
    let accepts_type = |candidate_role, required| match candidate_role {
        BehaviorCandidateRole::Subject => staff_subject_matches(definitions, role, required),
        BehaviorCandidateRole::Target => {
            required == AssetId::from_key("fromtoken")
                || (candidate_facts.target_is_request_site && required == AssetId::from_key("self"))
                || target_definition.is_some_and(|object| {
                    object.id == required || object.behavior_type_identifiers.contains(&required)
                })
        }
        BehaviorCandidateRole::Object => required == request_token,
    };
    index
        .candidates(request_token)
        .filter_map(|id| behavior_documents.find_behavior_task_location(id))
        .filter(|(_, _, task)| {
            (task.subjects.is_empty()
                || task.subjects.iter().any(|subject| {
                    accepts_type(
                        BehaviorCandidateRole::Subject,
                        AssetId::from_key(&subject.to_ascii_lowercase()),
                    )
                }))
                && (task.targets.is_empty()
                    || task.targets.iter().any(|target| {
                        accepts_type(
                            BehaviorCandidateRole::Target,
                            AssetId::from_key(&target.to_ascii_lowercase()),
                        )
                    }))
                && staff_behavior_task_satisfies_candidate_eligibility(
                    task,
                    candidate_facts,
                    accepts_type,
                )
        })
        .max_by(|(_, _, left), (_, _, right)| {
            left.priority
                .unwrap_or_default()
                .total_cmp(&right.priority.unwrap_or_default())
        })
        .map(
            |(document, declaration_index, definition)| CompatibleStaffBehaviorTask {
                document,
                declaration_index,
                definition,
            },
        )
}

#[cfg(test)]
pub(super) fn select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility<
    'a,
>(
    role: openzt2_game_data::world_definitions::staff_management::StaffRoleKind,
    candidates: impl Iterator<Item = CompatibleStaffBehaviorTask<'a>>,
    request_token: Option<AssetId>,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> Option<CompatibleStaffBehaviorTask<'a>> {
    candidates
        .filter(|candidate| {
            candidate.definition.subjects.iter().any(|subject| {
                authored_staff_role_subject_names(role)
                    .iter()
                    .any(|role| subject.eq_ignore_ascii_case(role))
            })
        })
        .filter(|candidate| {
            staff_behavior_task_satisfies_authored_request_token(
                candidate.definition,
                request_token,
            )
        })
        .filter(|candidate| {
            staff_behavior_task_satisfies_candidate_eligibility(
                candidate.definition,
                candidate_facts,
                |candidate_role, candidate| match candidate_role {
                    BehaviorCandidateRole::Subject => authored_staff_role_subject_names(role)
                        .iter()
                        .any(|name| candidate == AssetId::from_key(name)),
                    BehaviorCandidateRole::Target => {
                        candidate == AssetId::from_key("fromtoken")
                            || (candidate_facts.target_is_request_site
                                && candidate == AssetId::from_key("self"))
                    }
                    BehaviorCandidateRole::Object => true,
                },
            )
        })
        .max_by(|left, right| {
            left.definition
                .priority
                .unwrap_or_default()
                .total_cmp(&right.definition.priority.unwrap_or_default())
        })
}

fn staff_behavior_task_satisfies_candidate_eligibility(
    behavior_task: &BehaviorTask,
    candidate_facts: StaffCandidateEligibilityFacts,
    type_matches: impl Fn(BehaviorCandidateRole, AssetId) -> bool,
) -> bool {
    behavior_task
        .candidate_eligibility_requirements
        .iter()
        .all(|requirements| {
            candidate_requirement_group_is_satisfied(requirements, candidate_facts, &type_matches)
        })
}

fn candidate_requirement_group_is_satisfied(
    requirements: &BehaviorCandidateEligibilityRequirements,
    candidate_facts: StaffCandidateEligibilityFacts,
    type_matches: impl Fn(BehaviorCandidateRole, AssetId) -> bool,
) -> bool {
    if !candidate_type_identifiers_match(requirements, type_matches) {
        return false;
    }
    let mut results = requirements.eligibility_facts.iter().map(|fact| {
        evaluate_candidate_eligibility_fact(requirements.candidate_role, fact, candidate_facts)
    });
    match requirements.eligibility_fact_junction {
        openzt2_game_data::behavior::eligibility::BehaviorEligibilityFactJunction::All => {
            results.all(|result| result == Some(true))
        }
        openzt2_game_data::behavior::eligibility::BehaviorEligibilityFactJunction::Any => {
            requirements.eligibility_facts.is_empty() || results.any(|result| result == Some(true))
        }
    }
}

fn candidate_type_identifiers_match(
    requirements: &BehaviorCandidateEligibilityRequirements,
    type_matches: impl Fn(BehaviorCandidateRole, AssetId) -> bool,
) -> bool {
    if requirements.candidate_type_identifiers.is_empty() {
        return true;
    }
    let mut matches = requirements
        .candidate_type_identifiers
        .iter()
        .map(|&candidate| type_matches(requirements.candidate_role, candidate));
    match requirements.candidate_type_junction {
        BehaviorEligibilityFactJunction::All => matches.all(|matches| matches),
        BehaviorEligibilityFactJunction::Any => matches.any(|matches| matches),
    }
}

fn evaluate_candidate_eligibility_fact(
    candidate_role: BehaviorCandidateRole,
    fact: &BehaviorCandidateEligibilityFact,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> Option<bool> {
    let actual = match (candidate_role, fact.fact_input) {
        (
            _,
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::SupportedSurface),
        ) => Some(1 << 16), // Native inWater_OR_onLand is unconditional, unlike onLand/inWater.
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater),
        ) => candidate_facts
            .target_in_water
            .map(|value| i32::from(value) << 16),
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::OnLand),
        ) => candidate_facts
            .target_on_land
            .map(|value| i32::from(value) << 16),
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::Rampaging),
        ) => candidate_facts
            .target_rampaging
            .map(|value| i32::from(value) << 16),
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InShow),
        ) => candidate_facts
            .target_in_show
            .map(|value| i32::from(value) << 16),
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::FoodLevel),
        ) => candidate_facts.target_food_level_q16,
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::TrashLevel),
        ) => candidate_facts.target_trash_level_q16,
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::FilterDirtiness,
            ),
        ) => candidate_facts.target_filter_dirtiness_q16,
        _ => return None,
    }?;
    let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
        return None;
    };
    Some(match fact.comparison {
        BehaviorEligibilityFactComparison::Equal => actual == expected,
        BehaviorEligibilityFactComparison::NotEqual => actual != expected,
        BehaviorEligibilityFactComparison::Less => actual < expected,
        BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
        BehaviorEligibilityFactComparison::Greater => actual > expected,
        BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
    })
}

pub(super) fn staff_behavior_task_satisfies_authored_request_token(
    behavior_task: &BehaviorTask,
    request_token: Option<AssetId>,
) -> bool {
    let Some(request_token) = request_token else {
        return false;
    };
    behavior_task
        .objects
        .iter()
        .any(|authored_object_name| AssetId::from_key(authored_object_name) == request_token)
}

#[cfg(test)]
fn authored_staff_role_subject_names(
    role: openzt2_game_data::world_definitions::staff_management::StaffRoleKind,
) -> &'static [&'static str] {
    use openzt2_game_data::world_definitions::staff_management::StaffRoleKind;
    match role {
        StaffRoleKind::Keeper => &["keeper"],
        StaffRoleKind::Maintenance => &["worker"],
        StaffRoleKind::Educator => &["educator"],
        StaffRoleKind::Entertainer => &["entertainer"],
        StaffRoleKind::Trainer => &["trainer"],
        StaffRoleKind::Presenter => &["mc"],
        StaffRoleKind::Paleontologist => &["paleontologist"],
        StaffRoleKind::Recovery => &[
            "dinorecoverytranqteam",
            "dinorecoverycrateteam",
            "dinorecoveryrobot",
        ],
        StaffRoleKind::None | StaffRoleKind::Veterinarian => &[],
    }
}

pub(crate) fn staff_assignment_accepts_job_target(
    assigned_area: Option<&StaffAssignmentArea>,
    assigned_target: Option<&StaffAssignmentTarget>,
    target: Entity,
    target_parent: Option<Entity>,
    target_area: Option<Entity>,
) -> bool {
    assigned_target.is_none_or(|assigned| assigned.0 == target)
        && assigned_area.is_none_or(|area| {
            area.0 == target || Some(area.0) == target_parent || Some(area.0) == target_area
        })
}
