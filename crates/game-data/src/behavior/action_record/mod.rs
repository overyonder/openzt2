use serde::{Deserialize, Serialize};

use super::action;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorAction {
    Script {
        context: crate::AssetId,
        file: String,
        function: String,
        parameters: Vec<String>,
    },
    AnimationClip(action::animation::BehaviorAnimationClipAction),
    AttachObject(action::attach_object::BehaviorAttachObjectAction),
    Dock(action::docking::BehaviorDockAction),
    Move(action::movement::BehaviorMoveAction),
    WaitInteractionQueue(action::queue::BehaviorQueueWaitAction),
    SynchronizedSets(action::synchronized_sets::BehaviorSynchronizedSetsAction),
    EnterInteractionContainer {
        container: Option<crate::AssetId>,
        entrance_behavior_set: Option<crate::AssetId>,
    },
    Feedback(action::feedback::BehaviorFeedbackAction),
    ViewEvent(action::view_event::BehaviorViewEventAction),
    FactModifications(Vec<action::modification::BehaviorFactModification>),
    PlaySet(action::play_set::BehaviorPlaySetAction),
    TargetTest(action::target_test::BehaviorTargetTestAction),
    Termination(action::termination::BehaviorTerminationAction),
    Economy {
        transaction: crate::AssetId,
        cost_override: Option<f32>,
        tracker: Option<crate::AssetId>,
        general: bool,
    },
    RandomSet {
        weighted_sets: Vec<(crate::AssetId, f32)>,
        minimum_plays: u32,
        maximum_plays: u32,
        looping: bool,
    },
    RandomAnimation {
        weighted_clips: Vec<(String, f32)>,
        minimum_plays: u32,
        maximum_plays: u32,
        looping: bool,
    },
}
