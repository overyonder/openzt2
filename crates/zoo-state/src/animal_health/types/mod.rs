use bevy::prelude::*;
use openzt2_game_data::AssetId;

// Animal vitality, disease, and treatment state and requests.

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Vitality(pub f32);

impl Vitality {
    pub fn adjust_permille(&mut self, delta: i32) {
        self.0 = (self.0 + delta as f32 / 1000.0).clamp(0.0, 1.0);
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disease {
    pub definition: AssetId,
    pub elapsed_ticks: u64,
    pub severity_permille: u16,
    pub hint_level: u8,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Treatment {
    pub definition: AssetId,
    pub provider: Entity,
    pub remaining_ticks: u32,
}

// Tranquilization, escape, rampage, immobilization, and death state.

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tranquilized {
    pub remaining_ticks: u32,
    pub recovery_ticks: u32,
}

/// The animal has regained locomotion but remains in the authored recovery period.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveringFromTranquilizer {
    pub remaining_ticks: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escaped {
    pub since_tick: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rampaging {
    pub elapsed_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dead {
    pub cause: DeathCause,
    pub since_tick: u64,
}

/// An animal is immobilized by an authored activity until explicitly thawed.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frozen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathCause {
    Disease(AssetId),
}

/// Immobilize a live animal and cancel its current movement/activity.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezeAnimal {
    pub animal: Entity,
}

/// Return a frozen live animal to ordinary behavior selection.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThawAnimal {
    pub animal: Entity,
}

// Veterinary treatment, tranquilization, and capture requests.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreatmentRequest {
    pub animal: Entity,
    pub treatment: AssetId,
    pub provider: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranquilizeRequest {
    pub animal: Entity,
    pub tranquilizer: AssetId,
    pub source: Entity,
}

/// Requests that a staff member complete the recapture of an immobilized
/// escaped animal. The animal-health system validates and owns the health-state transition.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureAnimalRequest {
    pub animal: Entity,
    pub staff: Entity,
    pub job: Entity,
    pub execution_id: u64,
}

// Player tranquilizer-tool state, intent, presentation, and firing outcomes.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AimTranquilizer {
    pub controller: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FireTranquilizer {
    pub controller: Entity,
}

/// Player aim and charge state. The controller entity is also the shot source.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TranquilizerTool {
    pub target: Option<Entity>,
    pub tranquilizer: AssetId,
    pub charge_points: f32,
    pub required_charge_points: f32,
}

#[derive(Component, Debug)]
pub struct TranquilizerMisfireFeedback(pub Timer);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TranquilizerHud {
    pub distance_m: f32,
    pub in_range: bool,
    pub reticle: TranquilizerReticle,
    pub misfire_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranquilizerReticle {
    NoTarget,
    Charging,
    Ready,
    OutOfRange,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct TranquilizerFired {
    pub controller: Entity,
    pub tranquilizer: AssetId,
    pub outcome: TranquilizerFireOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranquilizerFireOutcome {
    Shot,
    Misfire,
    NoTarget,
    OutOfRange,
}

// Health-domain outcome messages consumed by other game domains.

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalDied {
    pub animal: Entity,
    pub cause: DeathCause,
}

/// A validated staff recapture cleared the animal's escaped response state.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalCaptured {
    pub animal: Entity,
    pub staff: Entity,
    pub job: Entity,
    pub execution_id: u64,
}
