use super::*;
use arrayvec::ArrayVec;
use openzt2_game_data::{
    behavior::document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    AssetId,
};

#[test]
fn unsupported_nested_set_fails_instead_of_returning_success_to_its_parent() {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .insert_resource(ZooClock {
            tick: 1,
            absolute_day: 0,
            tick_in_day: 1,
        })
        .add_message::<BehaviorTaskFailed>()
        .add_message::<BehaviorTaskFinished>()
        .add_systems(
            Update,
            advance_behavior_tasks_through_return_failure_completion_and_final_outcome,
        );
    let program = AssetId::from_key("unsupported-child");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![
                BehaviorSet {
                    id: program,
                    name: "unsupported-child".into(),
                    subjects: vec![],
                    actions: BehaviorTaskActionPhase::Unsupported {
                        first_unsupported_action: "UnmappedAction".into(),
                    },
                },
                BehaviorSet {
                    id: AssetId::from_key("parent"),
                    name: "parent".into(),
                    subjects: vec![],
                    actions: BehaviorTaskActionPhase::Supported(vec![]),
                },
            ]),
        ));
    let mut task = BehaviorTaskExecutionState {
        execution_id: 1,
        program: AssetId::from_key("parent"),
        target: None,
        document: document.clone(),
        declaration: 1,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 0,
        repetitions: 0,
        next_action_tick: 0,
        interaction_slot: None,
        stack: ArrayVec::new(),
    };
    task.stack.push(task.create_return_frame_after_action(0));
    task.program = program;
    task.declaration = 0;
    let actor = app.world_mut().spawn(task).id();
    for tick in 1..=3 {
        app.world_mut().resource_mut::<ZooClock>().tick = tick;
        app.update();
    }
    assert!(app
        .world()
        .get::<BehaviorTaskExecutionState>(actor)
        .is_none());
    assert_eq!(
        app.world().resource::<Messages<BehaviorTaskFailed>>().len(),
        1
    );
    assert!(app
        .world()
        .resource::<Messages<BehaviorTaskFinished>>()
        .is_empty());
}
