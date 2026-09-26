//! Data used by photo capture, screenshot capture, and captured evidence.

use bevy::prelude::*;
use openzt2_game_data::{species::LifeStage, AssetId as DomainAssetId};

use crate::plugins::world_spawn::persistent_id_types::PersistentIdError;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PhotoMode;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PhotoView {
    #[default]
    GroundCamera,
}

/// Visual facts recorded when the shutter fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoSubjectFacts {
    pub screen_permille: u16,
    /// Fraction of the projected collision rectangle inside the viewport.
    pub viewport_visible_permille: u16,
    pub center_permille: u16,
    pub facing_permille: u16,
    pub behavior_permille: u16,
    pub unoccluded: bool,
}

/// The subject's task and target when the shutter fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PhotoSubjectTaskEvidence {
    /// No task was running.
    NoCurrentTask,
    /// A task was running, but its name or target could not be resolved.
    UnresolvedAtCapture,
    /// Authored task name and target definition; `None` means no target.
    CurrentTask {
        authored_task_name: String,
        target_definition: Option<DomainAssetId>,
    },
}

/// Occupants of the subject's interaction container at capture, such as
/// guests seated on a bench. Habitat enclosure is recorded separately.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum PhotoSubjectContainedEntitiesEvidence {
    /// Occupancy or a member's kind was unavailable; contents are unknown.
    #[default]
    UnsupportedAtCapture,
    /// The container was empty.
    Empty,
    /// Occupants in admission order, excluding queue waiters.
    Contained(Vec<PhotoSubjectContainedEntityEvidence>),
}

/// One entity contained by a photographed subject at capture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoSubjectContainedEntityEvidence {
    /// The definition remains available to scorers after the entity is gone.
    pub entity: Entity,
    pub definition: DomainAssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotographableShowTrick(pub DomainAssetId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotographableFossil {
    pub set: DomainAssetId,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotographableHealth {
    pub health_milli: i32,
    pub diseased: bool,
    pub endangered: bool,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct Photo {
    pub image: Handle<Image>,
    pub captured_tick: u64,
    pub camera_position: Vec3,
    pub camera_rotation: Quat,
    pub score_milli: i32,
}

/// Definition used to look up the localized default caption.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoCaptionSubject(pub DomainAssetId);

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PhotoSubjects(pub Vec<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturePhotoRequest {
    pub camera: Entity,
}

/// A camera screenshot for export, without album membership or photo scoring.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureScreenshotRequest {
    pub camera: Entity,
}

/// The image produced by a screenshot request.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScreenshotCaptured {
    pub camera: Entity,
    pub image: Handle<Image>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenshotCaptureFailed {
    pub camera: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoCaptured {
    pub photo: Entity,
    pub score_milli: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoChallengeCompleted {
    pub photo: Entity,
    pub challenge: DomainAssetId,
    pub subject: DomainAssetId,
    pub rating_stars: u8,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoCaptureFailed {
    pub reason: PhotoFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhotoFailure {
    NotInPhotoMode,
    ReadbackBusy,
    InvalidCamera,
    CapacityExceeded,
    PersistentId(PersistentIdError),
    Gpu,
    Storage,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub(crate) struct PendingPhotoCapture {
    pub captured_tick: u64,
    pub camera_position: Vec3,
    pub camera_rotation: Quat,
    pub subjects: PhotoSubjects,
    pub view: PhotoView,
}

/// Prevents simultaneous captures of the same presented frame.
#[derive(Resource, Debug, Default)]
pub(crate) struct PhotoReadback {
    pub busy: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoCaptureOwner(pub Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenshotCaptureOwner(pub Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhotoEvidence {
    pub entity: Entity,
    pub definition: DomainAssetId,
    pub facts: PhotoSubjectFacts,
}

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PendingPhotoEvidence(pub Vec<PhotoEvidence>);

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CapturedPhotoEvidence(pub Vec<PhotoEvidence>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhotoSemanticEvidence {
    pub entity: Entity,
    pub position_mm: IVec3,
    /// Photo selectors can match a sex/life-stage variant as well as a species.
    pub animal_variant: Option<DomainAssetId>,
    pub behavior: Option<DomainAssetId>,
    pub animation: Option<String>,
    pub object_use: Option<DomainAssetId>,
    pub habitat: Option<DomainAssetId>,
    pub aquatic: bool,
    pub show_trick: Option<DomainAssetId>,
    pub fossil: Option<PhotographableFossil>,
    pub fossil_bone_level: Option<u16>,
    pub health: Option<PhotographableHealth>,
    pub life_stage: Option<LifeStage>,
    pub mother: Option<Entity>,
    pub father: Option<Entity>,
    /// Animal need values in `NeedKind` order.
    pub needs_q16: [Option<i32>; 10],
    pub super_animal: bool,
    pub named: bool,
    /// Habitat enclosure fact; unrelated to `contained_entities` below.
    pub contained: bool,
    /// Authored behavior-task name and target definition at shutter time.
    pub task: PhotoSubjectTaskEvidence,
    /// Authored interaction-container membership at shutter time.
    pub contained_entities: PhotoSubjectContainedEntitiesEvidence,
}

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CapturedPhotoSemantics(pub Vec<PhotoSemanticEvidence>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturedPhotoView(pub PhotoView);

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PendingPhotoSemantics(pub Vec<PhotoSemanticEvidence>);

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct PhotoSubjectsCollected;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct PendingPhotoScore;
