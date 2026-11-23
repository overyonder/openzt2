use serde::{Deserialize, Serialize};

use super::action;
use crate::{animation::animation_text_key::AuthoredAnimationTextCommand, AssetId};

pub type BehaviorAnimationTextKeyAliases = Vec<(AssetId, Vec<AuthoredAnimationTextCommand>)>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorAction {
    Script {
        context: crate::AssetId,
        file: String,
        function: String,
        parameters: Vec<String>,
    },
    DockTraversal {
        subject_node: String,
        target_path: String,
        clip: Option<String>,
    },
    AnimateTraversal {
        subject_node: String,
        target_path: String,
        exit: bool,
    },
    MorphEntity {
        recipient: BehaviorMorphRecipient,
        definition: crate::AssetId,
        name_it: bool,
        fade: bool,
        period: std::time::Duration,
    },
    AnimationClip(action::animation::BehaviorAnimationClipAction),
    AnimationSwitch(action::animation::BehaviorAnimationSwitchAction),
    AnimationWithTextKeys {
        animation: action::animation::BehaviorAnimationClipAction,
        keys: BehaviorAnimationTextKeyAliases,
    },
    AttachObject(action::attach_object::BehaviorAttachObjectAction),
    AttachWithTextKeys {
        attachment: action::attach_object::BehaviorAttachObjectAction,
        keys: BehaviorAnimationTextKeyAliases,
    },
    DetachObject(action::attach_object::BehaviorDetachObjectAction),
    DetachWithAnimation {
        detach: action::attach_object::BehaviorDetachObjectAction,
        animation: action::animation::BehaviorAnimationClipAction,
        keys: BehaviorAnimationTextKeyAliases,
    },
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
        #[serde(default)]
        pose: action::spawn::BehaviorSpawnPosePolicy,
    },
    /// `BFBehSpawn spawnRelation="child"`: a young animal of the `entity`
    /// variant beside the subject, which becomes its mother.
    SpawnChild {
        entity: crate::AssetId,
        #[serde(default)]
        pose: action::spawn::BehaviorSpawnPosePolicy,
    },
    SpawnChildWithPlacement {
        entity: AssetId,
        pose: action::spawn::BehaviorSpawnPosePolicy,
        fade_in: std::time::Duration,
        within_habitat: bool,
    },
    /// `BFBehSpawn` without a node or clip, such as the `TrashGround` a guest
    /// drops on a path: the object appears at once beside the actor.
    SpawnObjectAtActor {
        entity: crate::AssetId,
        #[serde(default)]
        pose: action::spawn::BehaviorSpawnPosePolicy,
    },
    Dock(action::docking::BehaviorDockAction),
    DockNow(action::specialized::BehaviorDockNowAction),
    DockSpline(action::specialized::BehaviorDockSplineAction),
    Fall(action::specialized::BehaviorFallAction),
    KickOffRider,
    AnimateForViewFromTransport { animation_type: String },
    FollowEntity(action::specialized::BehaviorFollowEntityAction),
    SplashGuest(action::specialized::BehaviorSplashGuestAction),
    UsePortal(action::specialized::BehaviorUsePortalAction),
    DockTankWall(action::specialized::BehaviorDockTankWallAction),
    StaffTrainAnimal(action::specialized::BehaviorStaffTrainAnimalAction),
    AddSubjectAsDiseaseTarget(action::specialized::BehaviorAddSubjectAsDiseaseTargetAction),
    InvalidAuthoredAction(String),
    DockWater(action::docking::BehaviorWaterDockAction),
    Move(action::movement::BehaviorMoveAction),
    Pursuit(action::movement::BehaviorPursuitAction),
    FaceTarget(action::face_target::BehaviorFaceTargetAction),
    ExitTankGate,
    Evasion(action::wander::BehaviorEvasionAction),
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
    PlaySetWithInterrupt {
        set: action::play_set::BehaviorPlaySetAction,
        interrupt: AssetId,
    },
    TargetTest(action::target_test::BehaviorTargetTestAction),
    Termination(action::termination::BehaviorTerminationAction),
    AnimationKill {
        animation: Option<action::animation::BehaviorAnimationClipAction>,
        termination: action::termination::BehaviorTerminationAction,
        radius: Option<(f32, AssetId)>,
        destroy: bool,
    },
    Whap {
        animation: Option<action::animation::BehaviorAnimationClipAction>,
        angle_degrees: Option<f32>,
        random_offset_degrees: Option<f32>,
    },
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
    SendSubjectTrigger(crate::AssetId),
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum BehaviorMorphRecipient {
    Subject,
    Target,
    NamedChild(crate::AssetId),
}
