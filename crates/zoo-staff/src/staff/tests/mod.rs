use bevy::prelude::*;
use openzt2_game_data::{
    behavior::{
        document::{BehaviorTask, BehaviorTaskActionPhase},
        eligibility::{
            BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
            BehaviorCandidateRole, BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
            BehaviorEligibilityFactJunction, BehaviorEligibilityFactValue,
            BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
        },
    },
    world_definitions::staff_management::{StaffJobKind, StaffRoleKind},
    AssetId,
};

use super::{
    staff_assignment_types::{StaffAssignmentArea, StaffAssignmentTarget},
    staff_employment_types::Staff,
    staff_job_claim_ranking::RankedStaffJobClaimCandidate,
    staff_job_completion_and_cancellation::{
        cancel_requested_jobs_and_jobs_with_invalid_world_relations,
        release_staff_and_despawn_completed_jobs,
    },
    staff_job_eligibility::{
        select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility,
        CompatibleStaffBehaviorTask, StaffCandidateEligibilityFacts,
    },
    staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs,
    staff_job_types::{CurrentJob, JobClaim, StaffJob, StaffJobRouting},
    staff_lifecycle_messages::{
        CancelStaffJobRequest, StaffJobCancelled, StaffJobCompleted, StaffJobRequest,
    },
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

#[test]
fn claim_order_is_urgency_priority_distance_then_entity() {
    let staff = Entity::from_bits(11);
    let low = RankedStaffJobClaimCandidate {
        job: Entity::from_bits(2),
        staff,
        urgency: 10,
        authored_priority: 3.0,
        distance_squared: 1.0,
    };
    let urgent = RankedStaffJobClaimCandidate {
        job: Entity::from_bits(3),
        staff,
        urgency: 11,
        authored_priority: -20.0,
        distance_squared: 100.0,
    };
    let priority = RankedStaffJobClaimCandidate {
        job: Entity::from_bits(4),
        staff,
        urgency: 10,
        authored_priority: 4.0,
        distance_squared: 100.0,
    };
    let mut candidates = [low, urgent, priority];
    candidates.sort_unstable_by(RankedStaffJobClaimCandidate::claim_order);
    assert_eq!(candidates, [urgent, priority, low]);
}

#[test]
fn duplicate_requests_create_one_job_per_routing() {
    let mut app = App::new();
    let root = app.world_mut().spawn_empty().id();
    let target = app.world_mut().spawn(WorldMember { root }).id();
    app.world_mut()
        .entity_mut(root)
        .insert(PersistentIdAllocator::default());
    app.add_message::<StaffJobRequest>()
        .add_systems(Update, create_or_raise_priority_of_requested_staff_jobs);
    let request = StaffJobRequest {
        kind: Some(StaffJobKind::EmptyBin),
        target,
        urgency: 50,
        token: None,
        routing: Default::default(),
    };
    app.world_mut().write_message(request);
    app.world_mut().write_message(request);
    app.world_mut().write_message(StaffJobRequest {
        routing: StaffJobRouting {
            subject_type: Some(AssetId::from_key("worker")),
            recipient: None,
        },
        ..request
    });
    app.update();
    let count = {
        let world = app.world_mut();
        let mut jobs = world.query::<&StaffJob>();
        jobs.iter(world).count()
    };
    assert_eq!(count, 2);
}

#[test]
fn deleted_assignment_relationships_are_cleared() {
    let mut app = App::new();
    let root = app.world_mut().spawn_empty().id();
    let area = app.world_mut().spawn(WorldMember { root }).id();
    let target = app.world_mut().spawn(WorldMember { root }).id();
    let staff = app
        .world_mut()
        .spawn((
            Staff,
            StaffAssignmentArea(area),
            StaffAssignmentTarget(target),
            WorldMember { root },
        ))
        .id();
    app.add_observer(
        super::staff_assignment_cleanup::release_staff_restrictions_when_target_leaves_world,
    );
    app.world_mut().despawn(area);
    app.world_mut().entity_mut(target).remove::<WorldMember>();
    app.update();
    assert!(app.world().get_entity(staff).is_ok());
    assert!(app.world().get::<StaffAssignmentArea>(staff).is_none());
    assert!(app.world().get::<StaffAssignmentTarget>(staff).is_none());
}

#[test]
fn completion_clears_reciprocal_facts_and_releases_staff() {
    let mut app = App::new();
    app.add_message::<StaffJobCompleted>()
        .add_systems(Update, release_staff_and_despawn_completed_jobs);
    let target = app.world_mut().spawn_empty().id();
    let staff = app.world_mut().spawn((Staff,)).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: Some(StaffJobKind::Feed),
                target,
                urgency: 1,
                token: None,
            },
            JobClaim { staff },
        ))
        .id();
    app.world_mut().write_message(StaffJobCompleted {
        staff,
        job,
        kind: StaffJobKind::Feed,
        target,
    });
    app.update();
    assert!(app.world().get_entity(job).is_err());
    assert!(app.world().get::<CurrentJob>(staff).is_none());
}

#[test]
fn missing_staff_releases_assignment_and_preserves_live_request() {
    let mut app = App::new();
    app.add_message::<CancelStaffJobRequest>()
        .add_message::<StaffJobCancelled>()
        .add_systems(
            Update,
            cancel_requested_jobs_and_jobs_with_invalid_world_relations,
        );
    let root = app.world_mut().spawn_empty().id();
    let target = app.world_mut().spawn(WorldMember { root }).id();
    let staff = app.world_mut().spawn((Staff, WorldMember { root })).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: Some(StaffJobKind::EmptyBin),
                target,
                urgency: 1,
                token: None,
            },
            JobClaim { staff },
        ))
        .id();
    app.world_mut().entity_mut(staff).despawn();
    app.update();
    assert!(app.world().get::<StaffJob>(job).is_some());
    assert!(app.world().get::<JobClaim>(job).is_none());
}

fn authored_keeper_feed_task(
    unique_identifier: &'static str,
    objects: &[&str],
    priority: i16,
) -> BehaviorTask {
    // `objects` holds post-loading canonical lowercase names, matching what
    // the behavior source loader stores in `BehaviorTask.objects`.
    BehaviorTask {
        id: AssetId::from_key(unique_identifier),
        name: unique_identifier.to_string(),
        priority: Some(f32::from(priority)),
        reservation_tag: None,
        sensor_tag: None,
        task_delay_seconds: None,
        subjects: vec!["keeper".to_string()],
        targets: vec!["fromtoken".to_string()],
        objects: objects.iter().map(|token| (*token).to_string()).collect(),
        candidate_eligibility_requirements: Vec::new(),
        scores: Vec::new(),
        execution: BehaviorTaskActionPhase::Supported(Vec::new()),
        completion: BehaviorTaskActionPhase::Supported(Vec::new()),
        failure: BehaviorTaskActionPhase::Supported(Vec::new()),
    }
}

fn candidate_for_task(task: &BehaviorTask) -> CompatibleStaffBehaviorTask<'_> {
    CompatibleStaffBehaviorTask {
        document: Handle::default(),
        declaration_index: 0,
        definition: task,
    }
}

fn select_from_candidate_pair<'a>(
    first: &'a BehaviorTask,
    second: &'a BehaviorTask,
    request_token: Option<AssetId>,
) -> Option<CompatibleStaffBehaviorTask<'a>> {
    select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
        StaffRoleKind::Keeper,
        [candidate_for_task(first), candidate_for_task(second)].into_iter(),
        request_token,
        StaffCandidateEligibilityFacts::default(),
    )
}

#[test]
fn authored_food_level_qualifier_compares_quantity_in_authored_points() {
    use openzt2_game_data::behavior::eligibility::BehaviorConditionEligibilityFact;

    let mut task =
        authored_keeper_feed_task("keeper:FillFoodContainer", &["t_fillfoodcontainer"], 10);
    task.candidate_eligibility_requirements = vec![BehaviorCandidateEligibilityRequirements {
        candidate_role: BehaviorCandidateRole::Target,
        eligibility_fact_junction: BehaviorEligibilityFactJunction::All,
        candidate_type_junction: BehaviorEligibilityFactJunction::Any,
        candidate_type_identifiers: vec![AssetId::from_key("fromtoken")],
        eligibility_facts: vec![BehaviorCandidateEligibilityFact {
            fact_input: BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::FoodLevel,
            ),
            comparison: BehaviorEligibilityFactComparison::LessOrEqual,
            expected_value: BehaviorEligibilityFactValue::Q16(33 << 16),
        }],
    }];
    for (quantity, eligible) in [
        (None, false),
        (Some(0), true),
        (Some(33 << 16), true),
        (Some(34 << 16), false),
    ] {
        let selected =
            select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
                StaffRoleKind::Keeper,
                [candidate_for_task(&task)].into_iter(),
                Some(AssetId::from_key("t_fillfoodcontainer")),
                StaffCandidateEligibilityFacts {
                    target_food_level_q16: quantity,
                    ..Default::default()
                },
            );
        assert_eq!(selected.is_some(), eligible);
    }
}

#[test]
fn candidate_requirements_gate_cure_land_water_and_unsupported_facts() {
    let boolean_fact = |fact_input| BehaviorCandidateEligibilityFact {
        fact_input,
        comparison: BehaviorEligibilityFactComparison::Equal,
        expected_value: BehaviorEligibilityFactValue::Q16(1 << 16),
    };
    let false_fact = |fact_input| BehaviorCandidateEligibilityFact {
        fact_input,
        comparison: BehaviorEligibilityFactComparison::Equal,
        expected_value: BehaviorEligibilityFactValue::Q16(0),
    };
    let requirements_for = |surface| {
        vec![
            BehaviorCandidateEligibilityRequirements {
                candidate_role: BehaviorCandidateRole::Subject,
                eligibility_fact_junction: BehaviorEligibilityFactJunction::All,
                candidate_type_junction: BehaviorEligibilityFactJunction::Any,
                candidate_type_identifiers: vec![AssetId::from_key("keeper")],
                eligibility_facts: vec![boolean_fact(BehaviorEligibilityFactInput::Spatial(
                    BehaviorSpatialEligibilityFact::SupportedSurface,
                ))],
            },
            BehaviorCandidateEligibilityRequirements {
                candidate_role: BehaviorCandidateRole::Target,
                eligibility_fact_junction: BehaviorEligibilityFactJunction::All,
                candidate_type_junction: BehaviorEligibilityFactJunction::Any,
                candidate_type_identifiers: vec![AssetId::from_key("fromtoken")],
                eligibility_facts: vec![
                    boolean_fact(BehaviorEligibilityFactInput::Spatial(surface)),
                    false_fact(BehaviorEligibilityFactInput::State(
                        BehaviorEntityStateEligibilityFact::Rampaging,
                    )),
                    false_fact(BehaviorEligibilityFactInput::State(
                        BehaviorEntityStateEligibilityFact::InShow,
                    )),
                ],
            },
        ]
    };

    let mut land = authored_keeper_feed_task("keeper:CureAnimal_Land", &["t_cureanimal"], 92);
    land.candidate_eligibility_requirements =
        requirements_for(BehaviorSpatialEligibilityFact::OnLand);
    let mut water = land.clone();
    water.id = AssetId::from_key("keeper:CureAnimal_Water");
    water.name = "keeper:CureAnimal_Water".to_string();
    water.candidate_eligibility_requirements =
        requirements_for(BehaviorSpatialEligibilityFact::InWater);

    let cure_token = Some(AssetId::from_key("t_cureanimal"));
    let land_facts = StaffCandidateEligibilityFacts {
        target_in_water: Some(false),
        target_on_land: Some(true),
        target_rampaging: Some(false),
        target_in_show: Some(false),
        target_food_level_q16: None,
        ..Default::default()
    };
    let selected = select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
        StaffRoleKind::Keeper,
        [candidate_for_task(&land), candidate_for_task(&water)].into_iter(),
        cure_token,
        land_facts,
    )
    .expect("land target selects CureAnimal_Land");
    assert_eq!(selected.definition.id, land.id);

    let mut rampaging_facts = land_facts;
    rampaging_facts.target_rampaging = Some(true);
    assert!(
        select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
            StaffRoleKind::Keeper,
            [candidate_for_task(&land), candidate_for_task(&water)].into_iter(),
            cure_token,
            rampaging_facts,
        )
        .is_none()
    );

    let mut unsupported_show_facts = land_facts;
    unsupported_show_facts.target_in_show = None;
    assert!(
        select_highest_priority_authored_staff_behavior_task_with_candidate_eligibility(
            StaffRoleKind::Keeper,
            [candidate_for_task(&land), candidate_for_task(&water)].into_iter(),
            cure_token,
            unsupported_show_facts,
        )
        .is_none()
    );
}

#[test]
fn request_token_selects_matching_authored_task_among_equal_priorities() {
    let fill_food_container =
        authored_keeper_feed_task("keeper:FillFoodContainer", &["t_fillfoodcontainer"], 10);
    let feed_animal = authored_keeper_feed_task("keeper:FeedAnimal", &["t_feedanimal"], 10);
    let fill_food_token = AssetId::from_key("t_fillfoodcontainer");
    let feed_animal_token = AssetId::from_key("t_feedanimal");

    // Deliberate both orderings: selection must not depend on candidate order
    // when priorities tie.
    for (first, second) in [
        (&fill_food_container, &feed_animal),
        (&feed_animal, &fill_food_container),
    ] {
        let selected = select_from_candidate_pair(first, second, Some(fill_food_token))
            .expect("dish token matches the dish refill task");
        assert_eq!(selected.definition.id, fill_food_container.id);

        let selected = select_from_candidate_pair(first, second, Some(feed_animal_token))
            .expect("animal token matches the hand-feeding task");
        assert_eq!(selected.definition.id, feed_animal.id);
    }
}

#[test]
fn higher_priority_mismatched_task_is_rejected_over_matching_task() {
    let fill_food_container =
        authored_keeper_feed_task("keeper:FillFoodContainer", &["t_fillfoodcontainer"], 5);
    let feed_animal = authored_keeper_feed_task("keeper:FeedAnimal", &["t_feedanimal"], 10);
    // A token-scoped request must not silently relax into a higher-priority
    // task whose Objects tokens do not contain the request token.
    let selected = select_from_candidate_pair(
        &fill_food_container,
        &feed_animal,
        Some(AssetId::from_key("t_fillfoodcontainer")),
    );
    assert_eq!(
        selected
            .expect("dish token claims the dish task")
            .definition
            .id,
        fill_food_container.id
    );
}

#[test]
fn distinct_tokens_and_tokenless_requests_dedupe_separately() {
    let mut app = App::new();
    let root = app.world_mut().spawn_empty().id();
    let target = app.world_mut().spawn(WorldMember { root }).id();
    app.world_mut()
        .entity_mut(root)
        .insert(PersistentIdAllocator::default());
    app.add_message::<StaffJobRequest>()
        .add_systems(Update, create_or_raise_priority_of_requested_staff_jobs);
    let fill_food_token = AssetId::from_key("t_fillfoodcontainer");
    let feed_animal_token = AssetId::from_key("t_feedanimal");
    for token in [Some(fill_food_token), Some(feed_animal_token), None] {
        app.world_mut().write_message(StaffJobRequest {
            kind: Some(StaffJobKind::Feed),
            target,
            urgency: 10,
            token,
            routing: Default::default(),
        });
    }
    // Same-batch duplicates coalesce to the maximum urgency before the
    // deferred spawn, so this request raises the dish job to 20 instead of
    // being discarded.
    app.world_mut().write_message(StaffJobRequest {
        kind: Some(StaffJobKind::Feed),
        target,
        urgency: 20,
        token: Some(fill_food_token),
        routing: Default::default(),
    });
    app.update();
    let mut jobs = {
        let world = app.world_mut();
        let mut jobs = world.query::<(&StaffJob, Option<&JobClaim>)>();
        jobs.iter(world)
            .map(|(job, claim)| (job.kind, job.token, job.urgency, claim.is_some()))
            .collect::<Vec<_>>()
    };
    jobs.sort_by_key(|(kind, token, urgency, _)| (kind.map(|kind| kind as u8), *token, *urgency));
    let mut expected = vec![
        (Some(StaffJobKind::Feed), None, 10, false),
        (Some(StaffJobKind::Feed), Some(fill_food_token), 20, false),
        (Some(StaffJobKind::Feed), Some(feed_animal_token), 10, false),
    ];
    expected
        .sort_by_key(|(kind, token, urgency, _)| (kind.map(|kind| kind as u8), *token, *urgency));
    assert_eq!(jobs, expected);
}

#[test]
fn same_batch_jobs_spawn_in_first_seen_message_order() {
    let mut app = App::new();
    let root = app.world_mut().spawn_empty().id();
    let animal_target = app.world_mut().spawn(WorldMember { root }).id();
    let dish_target = app.world_mut().spawn(WorldMember { root }).id();
    app.world_mut()
        .entity_mut(root)
        .insert(PersistentIdAllocator::default());
    app.add_message::<StaffJobRequest>()
        .add_systems(Update, create_or_raise_priority_of_requested_staff_jobs);
    // Write the animal feed request first, interleave its duplicate, then the
    // dish request. The duplicate raises urgency without changing the first
    // occurrence's PersistentId order.
    app.world_mut().write_message(StaffJobRequest {
        kind: Some(StaffJobKind::Feed),
        target: animal_target,
        urgency: 10,
        token: Some(AssetId::from_key("t_feedanimal")),
        routing: Default::default(),
    });
    app.world_mut().write_message(StaffJobRequest {
        kind: Some(StaffJobKind::Feed),
        target: dish_target,
        urgency: 10,
        token: Some(AssetId::from_key("t_fillfoodcontainer")),
        routing: Default::default(),
    });
    app.world_mut().write_message(StaffJobRequest {
        kind: Some(StaffJobKind::Feed),
        target: animal_target,
        urgency: 25,
        token: Some(AssetId::from_key("t_feedanimal")),
        routing: Default::default(),
    });
    app.update();
    let mut spawned_jobs = {
        let world = app.world_mut();
        let mut jobs = world.query::<(&StaffJob, &PersistentId)>();
        jobs.iter(world)
            .map(|(job, persistent_id)| (persistent_id.0, job.target, job.urgency))
            .collect::<Vec<_>>()
    };
    spawned_jobs.sort_by_key(|(persistent_id, _, _)| *persistent_id);
    assert_eq!(spawned_jobs.len(), 2);
    assert_eq!(
        spawned_jobs,
        vec![(1, animal_target, 25), (2, dish_target, 10),]
    );
}
