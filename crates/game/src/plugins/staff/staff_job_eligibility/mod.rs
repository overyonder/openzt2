use bevy::prelude::*;
use openzt2_game_data::{
    behavior::{
        document::BehaviorTask,
        eligibility::{
            BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
            BehaviorCandidateRole, BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
            BehaviorEligibilityFactJunction, BehaviorEligibilityFactValue,
            BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
        },
    },
    world_definitions::staff_management::{StaffJobKind, StaffRoleDefinition, StaffRoleKind},
    AssetId,
};

use crate::assets::behavior::behavior_asset_types::{
    BehaviorDeclarationIndexView, BehaviorDocumentAsset,
};

use super::staff_assignment_types::StaffAssignment;

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
    pub(super) target_in_water: Option<bool>,
    pub(super) target_on_land: Option<bool>,
    pub(super) target_rampaging: Option<bool>,
    pub(super) target_in_show: Option<bool>,
}

pub(super) fn find_highest_priority_compatible_staff_behavior_task<'a>(
    behavior_documents: BehaviorDeclarationIndexView<'a>,
    role: &StaffRoleDefinition,
    job_kind: StaffJobKind,
    request_token: Option<AssetId>,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> Option<CompatibleStaffBehaviorTask<'a>> {
    let request_token = request_token?;
    select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
        role.role,
        authored_staff_behavior_task_unique_identifiers(job_kind)
            .iter()
            .filter_map(|unique_identifier| {
                behavior_documents
                    .find_behavior_task_location(AssetId::from_key(unique_identifier))
                    .map(
                        |(document, declaration_index, definition)| CompatibleStaffBehaviorTask {
                            document,
                            declaration_index,
                            definition,
                        },
                    )
            }),
        Some(request_token),
        candidate_facts,
    )
}

pub(super) fn select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility<
    'a,
>(
    role: StaffRoleKind,
    candidates: impl Iterator<Item = CompatibleStaffBehaviorTask<'a>>,
    request_token: Option<AssetId>,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> Option<CompatibleStaffBehaviorTask<'a>> {
    candidates
        .filter(|candidate| staff_role_accepts_behavior_task(role, candidate.definition))
        .filter(|candidate| {
            staff_behavior_task_satisfies_authored_request_token(
                candidate.definition,
                request_token,
            )
        })
        .filter(|candidate| {
            staff_behavior_task_satisfies_candidate_eligibility(
                role,
                candidate.definition,
                candidate_facts,
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
    role: StaffRoleKind,
    behavior_task: &BehaviorTask,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> bool {
    behavior_task
        .candidate_eligibility_requirements
        .iter()
        .all(|requirements| {
            candidate_requirement_group_is_satisfied(role, requirements, candidate_facts)
        })
}

fn candidate_requirement_group_is_satisfied(
    role: StaffRoleKind,
    requirements: &BehaviorCandidateEligibilityRequirements,
    candidate_facts: StaffCandidateEligibilityFacts,
) -> bool {
    if !candidate_type_identifiers_match(role, requirements) {
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
    role: StaffRoleKind,
    requirements: &BehaviorCandidateEligibilityRequirements,
) -> bool {
    if requirements.candidate_type_identifiers.is_empty() {
        return true;
    }
    let mut matches = requirements
        .candidate_type_identifiers
        .iter()
        .map(|candidate| match requirements.candidate_role {
            BehaviorCandidateRole::Subject => authored_staff_role_subject_names(role)
                .iter()
                .any(|name| *candidate == AssetId::from_key(name)),
            BehaviorCandidateRole::Target => *candidate == AssetId::from_key("fromtoken"),
            BehaviorCandidateRole::Object => true,
        });
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
        ) => Some(true), // Native inWater_OR_onLand is unconditional, unlike onLand/inWater.
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater),
        ) => candidate_facts.target_in_water,
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::OnLand),
        ) => candidate_facts.target_on_land,
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::Rampaging),
        ) => candidate_facts.target_rampaging,
        (
            BehaviorCandidateRole::Target,
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InShow),
        ) => candidate_facts.target_in_show,
        _ => return None,
    }?;
    let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
        return None;
    };
    let actual = if actual { 1 << 16 } else { 0 };
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

/// Duty-capability inquiry for worker-duty assignment: asks only whether the
/// role has any authored behavior task for the job kind.  This is a separate
/// availability question from claim-time selection and deliberately applies
/// no request-token semantics, because no request token exists in that UI
/// context.
pub(super) fn staff_role_has_compatible_authored_behavior_task(
    behavior_documents: BehaviorDeclarationIndexView<'_>,
    role: &StaffRoleDefinition,
    job_kind: StaffJobKind,
) -> bool {
    authored_staff_behavior_task_unique_identifiers(job_kind)
        .iter()
        .filter_map(|unique_identifier| {
            behavior_documents.find_behavior_task_location(AssetId::from_key(unique_identifier))
        })
        .any(|(_, _, behavior_task)| staff_role_accepts_behavior_task(role.role, behavior_task))
}

fn staff_role_accepts_behavior_task(role: StaffRoleKind, behavior_task: &BehaviorTask) -> bool {
    authored_staff_role_subject_names(role)
        .iter()
        .any(|role_name| {
            behavior_task
                .subjects
                .iter()
                .any(|subject_name| subject_name.eq_ignore_ascii_case(role_name))
        })
}

fn authored_staff_role_subject_names(role: StaffRoleKind) -> &'static [&'static str] {
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

fn authored_staff_behavior_task_unique_identifiers(
    job_kind: StaffJobKind,
) -> &'static [&'static str] {
    match job_kind {
        StaffJobKind::Feed => &["keeper:fillfoodcontainer", "keeper:feedanimal"],
        StaffJobKind::RefillWater => &["keeper:wateranimal"],
        StaffJobKind::CleanHabitat => &["worker:rakepoo"],
        StaffJobKind::EmptyBin => &["worker:emptytrash", "worker:emptyrecycling"],
        StaffJobKind::SweepLitter => &["worker:sweeptrash"],
        StaffJobKind::Repair => &[],
        StaffJobKind::Treat => &[
            "keeper:healanimal_land",
            "keeper:healanimal_water",
            "keeper:cureanimal_land",
            "keeper:cureanimal_water",
        ],
        StaffJobKind::Educate => &[
            "educator:educatepodium",
            "educator:educatefossileducationcenter",
        ],
        StaffJobKind::Entertain => &[
            "entertainer:entertainboredguest",
            "entertainer:entertainboredyoungguest",
        ],
        StaffJobKind::Tranquilize => &["dinorecoverytranqteam:tranqdinosaur"],
        StaffJobKind::Capture => &["dinorecoverycrateteam:cratedinosaur", "keeper:crateanimal"],
        StaffJobKind::MaintainTank => &["worker:cleanwaterfilter", "worker:cleantank"],
        StaffJobKind::OperateShow => &[
            "trainer:performshow",
            "mc:performshow",
            "entertainer:putonshow",
        ],
    }
}

pub(crate) fn staff_assignment_accepts_job_target(
    assignment: &StaffAssignment,
    target: Entity,
    target_parent: Option<Entity>,
    target_area: Option<Entity>,
) -> bool {
    assignment.target.is_none_or(|assigned| assigned == target)
        && assignment.area.is_none_or(|area| {
            area == target || Some(area) == target_parent || Some(area) == target_area
        })
}
