use bevy::prelude::*;
use openzt2_game_data::AssetId;

// Animal vitality, disease, and treatment state and requests.

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vitality(pub f32);

impl Vitality {
    pub(crate) fn adjust_permille(&mut self, delta: i32) {
        self.0 = (self.0 + delta as f32 / 1000.0).clamp(0.0, 1.0);
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Disease {
    pub(crate) definition: AssetId,
    pub(crate) elapsed_ticks: u64,
    pub(crate) severity_permille: u16,
    pub(crate) hint_level: u8,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Treatment {
    pub(crate) definition: AssetId,
    pub(crate) provider: Entity,
    pub(crate) remaining_ticks: u32,
}

// Tranquilization, escape, rampage, immobilization, and death state.

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tranquilized {
    pub(crate) remaining_ticks: u32,
    pub(crate) recovery_ticks: u32,
}

/// The animal has regained locomotion but remains in the authored recovery period.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecoveringFromTranquilizer {
    pub(crate) remaining_ticks: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Escaped {
    pub(crate) since_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rampaging {
    pub(crate) elapsed_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dead {
    pub(crate) cause: DeathCause,
    pub(crate) since_tick: u64,
}

/// An animal is immobilized by an authored activity until explicitly thawed.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Frozen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeathCause {
    Disease(AssetId),
}

/// Immobilize a live animal and cancel its current movement/activity.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FreezeAnimal {
    pub(crate) animal: Entity,
}

/// Return a frozen live animal to ordinary behavior selection.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThawAnimal {
    pub(crate) animal: Entity,
}

// Veterinary treatment, tranquilization, and capture requests.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreatmentRequest {
    pub(crate) animal: Entity,
    pub(crate) treatment: AssetId,
    pub(crate) provider: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranquilizeRequest {
    pub(crate) animal: Entity,
    pub(crate) tranquilizer: AssetId,
    pub(crate) source: Entity,
}

/// Requests that a staff member complete the recapture of an immobilized
/// escaped animal. The animal-health system validates and owns the health-state transition.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureAnimalRequest {
    pub(crate) animal: Entity,
    pub(crate) staff: Entity,
}

// Player tranquilizer-tool state, intent, presentation, and firing outcomes.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AimTranquilizer {
    pub(crate) controller: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FireTranquilizer {
    pub(crate) controller: Entity,
}

/// Player aim and charge state. The controller entity is also the shot source.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TranquilizerTool {
    pub(crate) target: Option<Entity>,
    pub(crate) tranquilizer: AssetId,
    pub(crate) charge_points: f32,
    pub(crate) required_charge_points: f32,
}

#[derive(Component, Debug)]
pub(crate) struct TranquilizerMisfireFeedback(pub Timer);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TranquilizerHud {
    pub(crate) distance_m: f32,
    pub(crate) in_range: bool,
    pub(crate) reticle: TranquilizerReticle,
    pub(crate) misfire_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranquilizerReticle {
    NoTarget,
    Charging,
    Ready,
    OutOfRange,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TranquilizerFired {
    pub(crate) controller: Entity,
    pub(crate) tranquilizer: AssetId,
    pub(crate) outcome: TranquilizerFireOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranquilizerFireOutcome {
    Shot,
    Misfire,
    NoTarget,
    OutOfRange,
}

// Health-domain outcome messages consumed by other game domains.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalDied {
    pub(crate) animal: Entity,
    pub(crate) cause: DeathCause,
}

/// A validated staff recapture cleared the animal's escaped response state.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalCaptured {
    pub(crate) animal: Entity,
    pub(crate) staff: Entity,
}
