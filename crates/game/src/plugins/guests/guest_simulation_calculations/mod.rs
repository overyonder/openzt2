use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestMemoryKind, GuestVisitPurpose, MemoryReplacement,
};

use super::guest_simulation_types::{GuestMemories, GuestMemoryEntry};
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;

pub(crate) fn advance_guest_need_adjustment_timer(
    remaining_ns: &mut u64,
    step_ns: u64,
    delay_steps: u32,
    rng: &mut DeterministicRng,
) -> bool {
    *remaining_ns = remaining_ns.saturating_sub(step_ns);
    if *remaining_ns != 0 {
        return false;
    }
    let Some(delay) = rng.range_u32(delay_steps) else {
        return false;
    };
    // The native thinker chooses [0, 2 * StateThinkerInterval) and
    // multiplies by 100 / 1000 seconds. A zero delay expires next tick.
    *remaining_ns = u64::from(delay) * 100_000_000;
    true
}

pub(crate) const PERMILLE_MAX: u16 = 1_000;
const Q16_ONE: i64 = 1 << 16;

pub(crate) fn apply_q16(value: &mut u16, residual_q16: &mut u16, rate_q16: i32) {
    let total = i64::from(*value) * Q16_ONE + i64::from(*residual_q16) + i64::from(rate_q16);
    let maximum = i64::from(PERMILLE_MAX) * Q16_ONE;
    let clamped = total.clamp(0, maximum);
    *value = (clamped / Q16_ONE) as u16;
    *residual_q16 = if clamped == 0 || clamped == maximum {
        0
    } else {
        (clamped % Q16_ONE) as u16
    };
}

pub(crate) fn apply_permille_delta(value: u16, delta: i16) -> u16 {
    (i32::from(value) + i32::from(delta)).clamp(0, i32::from(PERMILLE_MAX)) as u16
}

pub(crate) fn survey_rate_permille(points: u32, maximum: u32) -> u16 {
    if maximum == 0 {
        return 0;
    }
    (u64::from(points.min(maximum)) * u64::from(PERMILLE_MAX) / u64::from(maximum)) as u16
}

pub(crate) fn survey_view_score_permille(
    critical_need_rate: u16,
    education_view_rate: u16,
    entertainment_view_rate: u16,
    arrived: bool,
) -> u16 {
    let average_view_rate =
        (u32::from(education_view_rate) + u32::from(entertainment_view_rate)) / 2;
    let arrived_rate = if arrived { PERMILLE_MAX } else { 0 };
    ((u32::from(critical_need_rate) + average_view_rate + u32::from(arrived_rate)) / 3) as u16
}

pub(crate) fn push_memory(
    memories: &mut GuestMemories,
    entry: GuestMemoryEntry,
    replacement: MemoryReplacement,
) {
    let capacity = usize::from(memories.active_capacity)
        .min(openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY);
    if capacity == 0 {
        memories.entries.clear();
        memories.next = 0;
        return;
    }

    if let Some(existing) = memories
        .entries
        .iter_mut()
        .find(|existing| existing.subject == entry.subject && existing.kind == entry.kind)
    {
        *existing = entry;
        return;
    }
    if memories.entries.len() < capacity {
        memories.entries.push(entry);
        memories.next = (memories.entries.len() % capacity) as u16;
        return;
    }

    let slot = match replacement {
        MemoryReplacement::Oldest => usize::from(memories.next) % capacity,
        MemoryReplacement::LowestAbsoluteValue => memories
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(index, entry)| (entry.value_permille.unsigned_abs(), *index))
            .map(|(index, _)| index)
            .unwrap_or(0),
    };
    memories.entries[slot] = entry;
    memories.next = ((slot + 1) % capacity) as u16;
}

pub(crate) fn memory_value(kind: GuestMemoryKind, satisfaction: i16, education: i16) -> i16 {
    match kind {
        GuestMemoryKind::Education => education,
        _ => satisfaction,
    }
}

pub(crate) fn inclusive_u32(low: u32, high: u32, random: u32) -> u32 {
    let width = high.saturating_sub(low).saturating_add(1);
    low.saturating_add(random % width.max(1))
}

pub(crate) fn should_leave(
    satisfaction: u16,
    hunger: u16,
    thirst: u16,
    energy: u16,
    restroom: u16,
) -> bool {
    satisfaction == 0 || hunger == 0 || thirst == 0 || energy == 0 || restroom == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReachedAction {
    BeginViewing,
    RequestService,
    CompleteExit,
    Invalid,
}

pub(crate) fn reached_action(purpose: GuestVisitPurpose, visible: bool) -> ReachedAction {
    if !visible {
        return ReachedAction::Invalid;
    }
    match purpose {
        GuestVisitPurpose::View => ReachedAction::BeginViewing,
        GuestVisitPurpose::Exit => ReachedAction::CompleteExit,
        GuestVisitPurpose::Food
        | GuestVisitPurpose::Drink
        | GuestVisitPurpose::Restroom
        | GuestVisitPurpose::Rest
        | GuestVisitPurpose::Education
        | GuestVisitPurpose::Shop
        | GuestVisitPurpose::Show
        | GuestVisitPurpose::Tour => ReachedAction::RequestService,
    }
}

#[cfg(test)]
mod guest_need_adjustment_timer_tests {
    use super::*;

    #[test]
    fn guest_need_adjustment_waits_without_consuming_randomness() {
        let mut rng = DeterministicRng::from_raw([42, 3]);
        let original_rng = rng;
        let mut remaining = 300_000_000;
        for _ in 0..2 {
            assert!(!advance_guest_need_adjustment_timer(
                &mut remaining,
                100_000_000,
                60,
                &mut rng,
            ));
            assert_eq!(rng, original_rng);
        }
        assert!(advance_guest_need_adjustment_timer(
            &mut remaining,
            100_000_000,
            60,
            &mut rng,
        ));
        assert!(remaining < 6_000_000_000);
        assert_eq!(remaining % 100_000_000, 0);
        assert_ne!(rng, original_rng);
    }
}
