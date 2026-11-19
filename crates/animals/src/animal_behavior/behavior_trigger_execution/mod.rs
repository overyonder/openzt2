use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{world_entity_crating::WorldEntityIsCrated, world_membership_types::WorldMember},
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorSendSubjectTrigger, RunningBehaviorInstruction,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_set_start_request_types::PendingBehaviorSet,
    animal_behavior::behavior_trigger_types::{BehaviorSubjectTrigger, BehaviorTriggerDelivery},
    animal_health::types::{Dead, Frozen, Tranquilized},
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure, SynchronizedBehaviorParticipant,
    },
};

pub(super) fn register_trigger_execution(app: &mut App) {
    app.add_message::<BehaviorSubjectTrigger>().add_systems(
        FixedUpdate,
        deliver_current_subject_triggers
            .in_set(FixedGameSet::Think)
            .in_set(BehaviorTriggerDelivery)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn deliver_current_subject_triggers(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorSendSubjectTrigger,
            ),
        ),
        (
            With<WorldMember>,
            Without<PendingBehaviorTaskFailure>,
            Without<Dead>,
            Without<Frozen>,
            Without<Tranquilized>,
            Without<WorldEntityIsCrated>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorSet>,
        ),
    >,
    mut triggers: MessageWriter<BehaviorSubjectTrigger>,
) {
    if !active
        .get(&definitions)
        .and_then(|definitions| definitions.behavior_selection_policy())
        .is_some_and(|policy| policy.enabled)
    {
        return;
    }
    for (subject, (task, instruction, running_action)) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let trigger = &running_action.0;
        triggers.write(BehaviorSubjectTrigger {
            subject,
            trigger: *trigger,
        });
        instruction.complete(subject, clock.tick, &mut commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
    use openzt2_game_data::AssetId;
    use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;

    #[test]
    fn trigger_delivery_advances_once_and_keeps_the_following_instruction_for_the_next_tick() {
        let mut app = App::new();
        app.add_observer(openzt2_zoo_state::behavior_task_execution_types::running_actions::apply_behavior_instruction_completion);
        app.init_resource::<Assets<BehaviorDocumentAsset>>()
            .init_resource::<Assets<WorldDefinitionAsset>>()
            .insert_resource(ZooClock {
                tick: 7,
                fractional_tick: Default::default(),
                absolute_day: 0,
                tick_in_day: 7,
            })
            .add_message::<BehaviorSubjectTrigger>()
            .add_systems(Update, deliver_current_subject_triggers);
        let mut definition =
            openzt2_game_data::world_definitions::document::WorldDefinitionDocument::default();
        definition.behavior_selection_policy = Some(openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy {
            enabled: true, minimum_score: 0.0, outside_range_need_value: 0.0,
            rejected_target_retry_interval: 1, rejected_target_retry_interval_cap: 1,
        });
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(definition.clone()));
        let mut active = WorldDefinitions::default();
        active.index_test_document(handle, &definition);
        app.insert_resource(active);
        let document = app.world_mut().resource_mut::<Assets<BehaviorDocumentAsset>>().add(
            BehaviorDocumentAsset::from_test_source(br#"<BehaviorSets><Leave><behaviors><BFBehSendTrigger subjectTrigger="Adjust_SurveyData"/><BFBehSendTrigger subjectTrigger="Adjust_ZooFame"/></behaviors></Leave></BehaviorSets>"#),
        );
        let actor = app
            .world_mut()
            .spawn(BehaviorTaskExecutionState {
                execution_id: 1,
                program: AssetId::from_key("leave"),
                target: None,
                object: None,
                document,
                declaration: 0,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                interaction_slot: None,
                depth: 0,
            })
            .id();
        app.world_mut().entity_mut(actor).insert((
            WorldMember {
                root: Entity::PLACEHOLDER,
            },
            Frozen,
        ));
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            0
        );
        assert!(app
            .world()
            .resource::<Messages<BehaviorSubjectTrigger>>()
            .is_empty());
        app.world_mut().entity_mut(actor).remove::<Frozen>();
        app.update();
        let task = app
            .world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap();
        assert_eq!(task.action, 1);
        assert_eq!(task.next_action_tick, 8);
        let delivered = app
            .world_mut()
            .resource_mut::<Messages<BehaviorSubjectTrigger>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].subject, actor);
        assert_eq!(delivered[0].trigger, AssetId::from_key("adjust_surveydata"));
        app.update();
        assert!(app
            .world()
            .resource::<Messages<BehaviorSubjectTrigger>>()
            .is_empty());
        app.world_mut().resource_mut::<ZooClock>().tick = 8;
        app.update();
        let delivered = app
            .world_mut()
            .resource_mut::<Messages<BehaviorSubjectTrigger>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].trigger, AssetId::from_key("adjust_zoofame"));
    }
}
