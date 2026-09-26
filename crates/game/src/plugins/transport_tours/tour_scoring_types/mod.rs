use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Authoritative presentation-independent rating for one station.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StationRatingPermille(pub(crate) u16);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TourScore {
    pub(crate) value: f32,
    pub(crate) observations: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TourViewable {
    pub(super) definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransportTripCompleted {
    pub(crate) guest: Entity,
    pub(crate) circuit: Entity,
    pub(crate) score: f32,
    pub(crate) rating: f32,
}

/// A compact rider-local history. It is a gameplay fact, not a passenger list.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TourObservationMemory {
    subjects: [Entity; openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY],
    len: u8,
    next: u8,
}

impl Default for TourObservationMemory {
    fn default() -> Self {
        Self {
            subjects: [Entity::PLACEHOLDER;
                openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY],
            len: 0,
            next: 0,
        }
    }
}

impl TourObservationMemory {
    pub(super) fn observe_with_limit(
        &mut self,
        observed_subject: Entity,
        authored_subject_limit: u16,
    ) -> bool {
        let subject_capacity = self.subjects.len().min(usize::from(authored_subject_limit));
        if subject_capacity == 0 {
            return false;
        }
        let active_subject_count = usize::from(self.len).min(subject_capacity);
        if self.subjects[..active_subject_count].contains(&observed_subject) {
            return false;
        }
        if usize::from(self.len) > subject_capacity {
            self.len = subject_capacity as u8;
            self.next = (usize::from(self.next) % subject_capacity) as u8;
        }
        if usize::from(self.len) < subject_capacity {
            self.subjects[usize::from(self.len)] = observed_subject;
            self.len += 1;
        } else {
            let replacement_index = usize::from(self.next) % subject_capacity;
            self.subjects[replacement_index] = observed_subject;
            self.next = ((replacement_index + 1) % subject_capacity) as u8;
        }
        true
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TourObservationCadence {
    pub(super) ticks_until_next: u32,
}
