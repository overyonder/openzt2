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
    DetachObject(action::attach_object::BehaviorDetachObjectAction),
    /// `BFBehHide`: the actor is out of sight, as inside a gift shop, for a
    /// duration drawn once between these bounds.
    Hide {
        minimum: std::time::Duration,
        maximum: std::time::Duration,
    },
    SpawnObject {
        entity: crate::AssetId,
        subject_node: String,
        animation: String,
    },
    Dock(action::docking::BehaviorDockAction),
    DockWater(action::docking::BehaviorWaterDockAction),
    Move(action::movement::BehaviorMoveAction),
    WaitInteractionQueue(action::queue::BehaviorQueueWaitAction),
    DockQueue(action::queue::BehaviorDockQueueAction),
    SynchronizedSets(action::synchronized_sets::BehaviorSynchronizedSetsAction),
    EnterInteractionContainer {
        container: Option<crate::AssetId>,
        entrance_behavior_set: Option<crate::AssetId>,
    },
    Feedback(action::feedback::BehaviorFeedbackAction),
    ViewAnimals(action::view_animals::BehaviorViewAnimalsAction),
    ViewEvent(action::view_event::BehaviorViewEventAction),
    WatchShow(action::watch_show::BehaviorWatchShowAction),
    FactModifications(Vec<action::modification::BehaviorFactModification>),
    LearnTrick {
        trick: crate::AssetId,
        amount: u16,
        cap: u16,
    },
    CureDisease,
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
    PlaceTarget(action::place_target::BehaviorPlaceTargetAction),
    LocomotionSwitchSet(action::locomotion_switch_set::BehaviorLocomotionSwitchSetAction),
    AnimationSwitchSet(action::locomotion_switch_set::BehaviorAnimationSwitchSetAction),
    AnimalShowEvent(action::show_event::BehaviorShowEventAction),
    HeadLook(action::head_look::BehaviorHeadLookAction),
    Wander(action::wander::BehaviorWanderAction),
    SendSelfTokens(Vec<action::token::BehaviorSelfTokenDelivery>),
}
