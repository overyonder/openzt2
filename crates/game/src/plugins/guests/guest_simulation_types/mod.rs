use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestMemoryKind, GuestVisitPurpose,
};
use openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY;
use openzt2_game_data::AssetId;

use crate::plugins::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::persistent_id_types::PersistentIdError,
};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Guest;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestArchetype(pub AssetId);

/// The species this guest prefers to view. The original stores this fact on
/// each guest's view-animals capability; it is not state owned by the
/// favourite-animal trend selector.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestFavouriteAnimal(pub Option<AssetId>);

#[derive(Component, Debug, Clone, Copy)]
pub struct GuestRng(pub DeterministicRng);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GuestPhase {
    #[default]
    Arriving,
    Visiting,
    Leaving,
}

macro_rules! guest_need {
    ($name:ident) => {
        #[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name {
            pub value: u16,
            pub residual_q16: u16,
        }
    };
}

guest_need!(GuestHunger);
guest_need!(GuestThirst);
guest_need!(GuestDessert);
guest_need!(GuestGift);
guest_need!(GuestEnergy);
guest_need!(GuestRestroom);
guest_need!(GuestSocial);
guest_need!(GuestAmusement);

/// Canonical viewanimals deprivation in the authored signed source units.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestViewingNeed {
    pub(crate) value_q16: i32,
}

impl GuestViewingNeed {
    pub(crate) fn new(value_q16: i32) -> Self {
        Self {
            value_q16: value_q16.clamp(0, 100 << 16),
        }
    }

    pub(crate) fn add_source_delta(&mut self, delta_q16: i32) {
        // State values are clamped; thresholds may exceed these bounds.
        self.value_q16 = self.value_q16.saturating_add(delta_q16).clamp(0, 100 << 16);
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmusementPaintLevel(pub i32);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestAmusement {
    pub guest: Entity,
    pub delta_q16: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestHunger {
    pub guest: Entity,
    pub delta_q16: i32,
}
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestThirst {
    pub guest: Entity,
    pub delta_q16: i32,
}
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestRest {
    pub guest: Entity,
    pub delta_q16: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestRestroom {
    pub guest: Entity,
    pub delta_q16: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetAmusementPaintLevel {
    pub easel: Entity,
    pub level_q16: i32,
}

/// Canonical wellness permille plus its fractional Q16 remainder.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestSatisfaction(pub u16, pub(crate) u16);

impl GuestSatisfaction {
    pub(crate) fn from_source_q16(deprivation_q16: i32) -> Self {
        let mut satisfaction = Self(1_000, 0);
        satisfaction.add_wellness_q16(deprivation_q16.clamp(0, 100 << 16) * -10);
        satisfaction
    }

    pub(crate) fn source_q16(&self) -> i32 {
        let wellness = i32::from(self.0.min(1_000)) * (1 << 16) + i32::from(self.1);
        ((1_000 << 16) - wellness).clamp(0, 1_000 << 16) / 10
    }

    pub(crate) fn add_wellness_q16(&mut self, delta_q16: i32) {
        super::guest_simulation_calculations::apply_q16(&mut self.0, &mut self.1, delta_q16);
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestSatisfaction {
    pub guest: Entity,
    pub delta_q16: i32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestEducation(pub u16);

/// Authored f_departurePoints, in source units rather than wellness permille.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestDeparturePoints(pub(crate) i32);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisitTime {
    pub elapsed_ticks: u64,
    /// Fixed-clock nanoseconds until the next authored need adjustment.
    pub(crate) need_adjustment_remaining_ns: u64,
}

/// Terminal lifecycle handoff. Cleanup owners observe `GuestDeparted` and
/// release their relationships before the guest entity is actually despawned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeparturePending;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewing {
    pub subject: Entity,
    pub elapsed_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestDestination {
    pub entity: Entity,
    pub purpose: GuestVisitPurpose,
}

/// The navigation result currently awaited by guest arrival, activity, or
/// departure. Consuming the identifier immediately prevents repeated messages
/// from handing off the same arrival twice before deferred commands apply.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestNavigationRequest(pub(crate) Option<u64>);

/// One authored standing place selected for viewing one live animal.  Both
/// relationships belong to this guest entity; no parallel candidate list is
/// retained after the choice.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct GuestViewingTarget {
    pub subject: Entity,
    pub stand_position: Vec3,
}

/// Marks a constructed object whose definition contributes authored guest
/// standing slots. Slot coordinates remain borrowed from the loaded asset.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewingOpportunity;

/// Records that the loaded viewing policy has classified this inspectable.
/// This entity-local hydration fact prevents both missed late asset readiness
/// and repeated scans for objects that intentionally offer no viewing slots.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ViewingOpportunityClassified;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestMemoryEntry {
    pub subject: Entity,
    pub kind: GuestMemoryKind,
    pub value_permille: i16,
    pub age_ticks: u64,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GuestMemories {
    pub entries: ArrayVec<GuestMemoryEntry, GUEST_MEMORY_CAPACITY>,
    pub active_capacity: u16,
    pub next: u16,
}

impl GuestMemories {
    pub fn new(active_capacity: u16) -> Self {
        debug_assert!(usize::from(active_capacity) <= GUEST_MEMORY_CAPACITY);
        Self {
            entries: ArrayVec::new(),
            active_capacity: active_capacity.min(GUEST_MEMORY_CAPACITY as u16),
            next: 0,
        }
    }
}

impl Default for GuestMemories {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct GuestArrivalState {
    pub next_tick: u64,
    pub rng: DeterministicRng,
}

/// Current zoo-survey aggregate. This is derived simulation state, not an
/// entity registry or copied guest world.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestSurvey {
    pub next_tick: u64,
    pub month: u32,
    pub critical_hits: u16,
    pub education_points: u32,
    pub entertainment_points: u32,
    /// Remaining room beneath the authored monthly critical-need limit.
    pub critical_need_rate_permille: u16,
    /// Score contributed by the most recently applied education view.
    pub education_view_rate_permille: u16,
    /// Score contributed by the most recently applied entertainment view.
    pub entertainment_view_rate_permille: u16,
    /// Aggregate for the most recently applied view reaction.
    pub view_score_permille: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustGuestSurveyData {
    pub guest: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestReaction {
    pub guest: Entity,
    pub subject: Entity,
    pub kind: GuestMemoryKind,
    pub satisfaction_delta_permille: i16,
    pub education_delta_permille: i16,
}

/// Confirms that one reaction reached and mutated its canonical guest.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestReactionApplied {
    pub guest: Entity,
    pub subject: Entity,
    pub kind: GuestMemoryKind,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestArrived {
    pub guest: Entity,
}

/// Internal gate handoff to economy. Reaching the outside entrance is not yet an
/// admitted guest fact.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestReachedEntrance {
    pub guest: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestDeparted {
    pub guest: Entity,
    pub satisfaction_permille: u16,
    pub education_permille: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestArrivalFailed {
    pub(crate) reason: GuestArrivalFailure,
}

/// Requests an authored number of ordinary arriving guests. Scenario rewards
/// use the same generation, entrance, navigation, and initialization path as
/// naturally arriving visitors.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestGuestArrivals {
    pub count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuestArrivalFailure {
    MissingPolicy,
    MissingEntrance,
    InvalidDefinition(AssetId),
    PersistentId(PersistentIdError),
}
