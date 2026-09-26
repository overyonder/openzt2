use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::behavior::scalar::BehaviorScalarQ16;
use openzt2_game_data::{
    behavior::{
        action::{
            entity_role::BehaviorEntityRole,
            modification::{
                BehaviorFact, BehaviorFactModification, BehaviorFactModificationOperation,
            },
        },
        action_record::BehaviorAction,
        document::{BehaviorDocument, BehaviorTask, BehaviorTaskActionPhase},
    },
    AssetId,
};

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        animal_behavior::behavior_fact_modification::apply_supported_current_behavior_fact_modifications,
        animal_health::types::{FreezeAnimal, ThawAnimal},
        animal_welfare::types::{
            AdjustBathroom, AdjustExercise, AdjustHealthNeed, AdjustHunger, AdjustHygiene,
            AdjustPrivacy, AdjustRest, AdjustSocial, AdjustStimulation, AdjustThirst,
        },
        behavior_task_execution_types::{BehaviorTaskExecutionPhase, BehaviorTaskExecutionState},
        feeding::container_quantity::{FoodContainer, AUTHORED_CONTAINER_CAPACITY_Q16},
        simulation_time::simulation_clock_types::ZooClock,
        staff::staff_employment_types::Staff,
    },
};

fn install_action_app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<AdjustHunger>()
        .add_message::<AdjustBathroom>()
        .add_message::<AdjustThirst>()
        .add_message::<AdjustRest>()
        .add_message::<AdjustPrivacy>()
        .add_message::<AdjustSocial>()
        .add_message::<AdjustExercise>()
        .add_message::<AdjustStimulation>()
        .add_message::<AdjustHealthNeed>()
        .add_message::<AdjustHygiene>()
        .add_message::<FreezeAnimal>()
        .add_message::<ThawAnimal>();
    let target = app
        .world_mut()
        .spawn(FoodContainer {
            food: AssetId::from_key("FoodDish_Meat"),
            amount_q16: 0,
            capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
        })
        .id();
    let actor = app.world_mut().spawn(Staff).id();
    (app, actor, target)
}

fn add_food_level_action(app: &mut App, delta_q16: i32) -> Handle<BehaviorDocumentAsset> {
    let action = BehaviorAction::FactModifications(vec![BehaviorFactModification {
        affected_entity_role: BehaviorEntityRole::Target,
        modified_fact: BehaviorFact::FoodLevel,
        modification_operation: BehaviorFactModificationOperation::Add,
        modification_value: BehaviorScalarQ16::FixedQ16(delta_q16),
    }]);
    let document = BehaviorDocument::Tasks(vec![BehaviorTask {
        id: AssetId::from_key("test:food"),
        name: "FoodLevel test".to_owned(),
        priority: None,
        task_delay_seconds: None,
        reservation_tag: None,
        subjects: Vec::new(),
        targets: Vec::new(),
        objects: Vec::new(),
        candidate_eligibility_requirements: Vec::new(),
        scores: Vec::new(),
        execution: BehaviorTaskActionPhase::Supported(Vec::new()),
        completion: BehaviorTaskActionPhase::Supported(vec![action]),
        failure: BehaviorTaskActionPhase::Supported(Vec::new()),
    }]);
    app.world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(document))
}

fn attach_task(
    app: &mut App,
    actor: Entity,
    target: Entity,
    document: Handle<BehaviorDocumentAsset>,
) {
    app.world_mut()
        .entity_mut(actor)
        .insert(BehaviorTaskExecutionState {
            execution_id: 1,
            program: AssetId::from_key("test:food"),
            target: Some(target),
            document,
            declaration: 0,
            phase: BehaviorTaskExecutionPhase::Completion,
            action: 0,
            repetitions: 0,
            next_action_tick: 0,
            interaction_slot: None,
            stack: ArrayVec::new(),
        });
}

#[test]
fn keeper_actor_reaches_and_completes_a_saturated_food_level_write() {
    let (mut app, actor, target) = install_action_app();
    let document = add_food_level_action(&mut app, 100 << 16);
    app.world_mut()
        .entity_mut(target)
        .get_mut::<FoodContainer>()
        .unwrap()
        .amount_q16 = AUTHORED_CONTAINER_CAPACITY_Q16;
    attach_task(&mut app, actor, target, document);
    app.add_systems(Update, apply_supported_current_behavior_fact_modifications);
    app.update();
    assert_eq!(
        app.world().get::<FoodContainer>(target).unwrap().amount_q16,
        AUTHORED_CONTAINER_CAPACITY_Q16
    );
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        1
    );
    assert!(app
        .world()
        .get::<crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure>(actor)
        .is_none());
}

#[test]
fn keeper_actor_applies_food_level_once_without_a_second_refill_pipeline() {
    let (mut app, actor, target) = install_action_app();
    let document = add_food_level_action(&mut app, 40 << 16);
    attach_task(&mut app, actor, target, document);
    app.add_systems(Update, apply_supported_current_behavior_fact_modifications);
    app.update();
    assert_eq!(
        app.world().get::<FoodContainer>(target).unwrap().amount_q16,
        40 << 16
    );
}

#[test]
fn keeper_actor_with_an_unbacked_food_level_target_fails_the_behavior() {
    let (mut app, actor, target) = install_action_app();
    app.world_mut().entity_mut(target).remove::<FoodContainer>();
    let document = add_food_level_action(&mut app, 40 << 16);
    attach_task(&mut app, actor, target, document);
    app.add_systems(Update, apply_supported_current_behavior_fact_modifications);
    app.update();
    assert!(app
        .world()
        .get::<crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure>(actor)
        .is_some());
}

#[test]
fn non_food_level_operation_still_requires_animal_owner() {
    let (mut app, actor, target) = install_action_app();
    let action = BehaviorAction::FactModifications(vec![BehaviorFactModification {
        affected_entity_role: BehaviorEntityRole::Target,
        modified_fact: BehaviorFact::Hunger,
        modification_operation: BehaviorFactModificationOperation::Add,
        modification_value: BehaviorScalarQ16::FixedQ16(-10 << 16),
    }]);
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Tasks(vec![BehaviorTask {
                id: AssetId::from_key("test:hunger"),
                name: "Hunger test".to_owned(),
                priority: None,
                task_delay_seconds: None,
                reservation_tag: None,
                subjects: Vec::new(),
                targets: Vec::new(),
                objects: Vec::new(),
                candidate_eligibility_requirements: Vec::new(),
                scores: Vec::new(),
                execution: BehaviorTaskActionPhase::Supported(Vec::new()),
                completion: BehaviorTaskActionPhase::Supported(vec![action]),
                failure: BehaviorTaskActionPhase::Supported(Vec::new()),
            }]),
        ));
    attach_task(&mut app, actor, target, document);
    app.add_systems(Update, apply_supported_current_behavior_fact_modifications);
    app.update();
    assert!(app
        .world()
        .get::<crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure>(actor)
        .is_some());
    assert_eq!(
        app.world().get::<FoodContainer>(target).unwrap().amount_q16,
        0
    );
}
