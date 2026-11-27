use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::{behavior::action::feedback::BehaviorFeedbackEntry, AssetId};
use openzt2_world_state::{
    simulation_time::{
        native_visual_random_stream::NativeVisualRandomStream, simulation_clock_types::ZooClock,
        simulation_control_types::SimulationPaused,
    },
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    guests::guest_simulation_types::{Guest, GuestDeparturePoints, GuestPhase, GuestSatisfaction},
    locomotion::locomotion_types::SpatialGrid,
};

#[derive(Component, Default)]
pub struct GuestInfluenceCells(pub HashMap<u32, Vec<GuestInfluenceEntry>>);

#[derive(Clone, Copy, PartialEq)]
pub struct GuestInfluenceEntry {
    pub category: AssetId,
    pub key: AssetId,
    pub intensity: f32,
}

#[derive(Component)]
#[relationship(relationship_target = GuestsWatchingInfluence)]
pub struct GuestInfluenceWatcherOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = GuestInfluenceWatcherOf)]
pub struct GuestsWatchingInfluence(Vec<Entity>);

#[derive(Component)]
pub struct GuestInfluenceWatcher(pub Vec<GuestInfluenceWatchRow>);

pub struct GuestInfluenceWatchRow {
    pub category: Option<AssetId>,
    pub update_seconds: f64,
    pub feedback_chance: u16,
    pub sensitivities: Vec<(AssetId, f32)>,
    pub ranges: Vec<GuestInfluenceRange>,
    pub next_due_seconds: f64,
}

pub struct GuestInfluenceRange {
    pub minimum: f32,
    pub maximum: f32,
    pub feedback: Vec<BehaviorFeedbackEntry>,
    pub happiness_adjustment_q16: i32,
    pub departure_adjustment_q16: i32,
}

#[derive(Message)]
pub struct GuestInfluenceFeedback {
    pub guest: Entity,
    pub entries: Vec<BehaviorFeedbackEntry>,
}

pub(super) fn present_guest_influence_emoticons(
    mut feedback: MessageReader<GuestInfluenceFeedback>,
    guests: Query<(), With<Guest>>,
    mut commands: Commands,
) {
    for event in feedback.read().filter(|event| guests.contains(event.guest)) {
        for entry in &event.entries {
            let BehaviorFeedbackEntry::Emoticon {
                emoticon_name,
                emoticon_scale,
                ..
            } = entry
            else {
                continue;
            };
            commands.trigger(
                openzt2_game_assets::effect::PlayAuthoredParticleDictionaryEffect {
                    effect: AssetId::from_key(&emoticon_name.to_ascii_lowercase()),
                    subject: event.guest,
                    attach: true,
                    inherit_parent_rotation: false,
                    transform: Transform::from_scale(Vec3::splat(emoticon_scale.unwrap_or(1.0))),
                },
            );
        }
    }
}

impl GuestInfluenceWatchRow {
    fn sample(
        &mut self,
        now: f64,
        entries: &[GuestInfluenceEntry],
        draw: impl FnOnce() -> u16,
    ) -> Option<(&GuestInfluenceRange, bool)> {
        let category = self.category?;
        if now < self.next_due_seconds {
            return None;
        }
        self.next_due_seconds = now + self.update_seconds;
        let score = entries
            .iter()
            .filter(|entry| entry.category == category)
            .fold(0.0, |score, entry| {
                let weight = self
                    .sensitivities
                    .iter()
                    .find(|&&(key, _)| key == entry.key)
                    .map_or(0.0, |&(_, value)| value);
                score + entry.intensity * weight
            });
        let range = self
            .ranges
            .iter()
            .find(|range| range.minimum <= score && score < range.maximum)?;
        Some((range, draw() % 100 < self.feedback_chance))
    }
}

pub(super) fn apply_due_guest_influence(
    mut guests: Query<
        (
            Entity,
            &GuestPhase,
            &GuestInfluenceWatcherOf,
            &GlobalTransform,
            &WorldMember,
            &mut GuestSatisfaction,
            &mut GuestDeparturePoints,
        ),
        With<Guest>,
    >,
    mut watchers: Query<&mut GuestInfluenceWatcher>,
    roots: Query<(
        &SpatialGrid,
        &ZooClock,
        Option<&SimulationPaused>,
        Option<&GuestInfluenceCells>,
        Option<&super::guest_influence_grid::GuestTerrainInfluence>,
    )>,
    time: Res<Time<Fixed>>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut feedback: MessageWriter<GuestInfluenceFeedback>,
    mut commands: Commands,
) {
    for (guest, phase, watcher, transform, member, mut happiness, mut departure) in &mut guests {
        if *phase != GuestPhase::Visiting {
            continue;
        }
        let Ok((grid, clock, pause, influence, terrain)) = roots.get(member.root) else {
            continue;
        };
        if pause.is_some_and(|pause| pause.0) {
            continue;
        }
        let Some(cell) = grid.cell_of(transform.translation().xz()) else {
            continue;
        };
        let Ok(mut rows) = watchers.get_mut(watcher.0) else {
            continue;
        };
        let now = clock.tick as f64 * time.timestep().as_secs_f64();
        let values = influence
            .and_then(|influence| influence.0.get(&cell))
            .map(Vec::as_slice)
            .or_else(|| terrain.map(super::guest_influence_grid::GuestTerrainInfluence::entries))
            .unwrap_or_default();
        for row in &mut rows.0 {
            let Some((range, present)) = row.sample(now, values, || random.next_u15()) else {
                continue;
            };
            if present {
                super::view_event_execution::deliver_view_thoughts(
                    &mut commands,
                    guest,
                    guest,
                    None,
                    None,
                    &range.feedback,
                );
                feedback.write(GuestInfluenceFeedback {
                    guest,
                    entries: range.feedback.clone(),
                });
            }
            happiness.add_wellness_q16(
                openzt2_zoo_state::need_adjustment::wellness_delta_from_authored_deprivation_q16(
                    range.happiness_adjustment_q16,
                ),
            );
            departure.0 = departure
                .0
                .saturating_add(range.departure_adjustment_q16)
                .max(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watcher() -> GuestInfluenceWatchRow {
        GuestInfluenceWatchRow {
            category: Some(AssetId::from_key("smell")),
            update_seconds: 5.0,
            feedback_chance: 50,
            sensitivities: vec![
                (AssetId::from_key("bad"), -10.0),
                (AssetId::from_key("good"), 10.0),
            ],
            ranges: vec![GuestInfluenceRange {
                minimum: -1_000_000.0,
                maximum: -3.0,
                feedback: Vec::new(),
                happiness_adjustment_q16: 3 << 16,
                departure_adjustment_q16: 1 << 16,
            }],
            next_due_seconds: 0.0,
        }
    }

    fn entry(key: &str, intensity: f32) -> GuestInfluenceEntry {
        GuestInfluenceEntry {
            category: AssetId::from_key("smell"),
            key: AssetId::from_key(key),
            intensity,
        }
    }

    #[test]
    fn feedback_chance_is_strict_and_subject_adjustments_do_not_depend_on_it() {
        let entries = [entry("bad", 0.5), entry("good", 0.1), entry("neutral", 1.0)];
        for (roll, expected) in [(49, true), (50, false)] {
            let mut watcher = watcher();
            let (range, present) = watcher.sample(10.0, &entries, || roll).unwrap();
            assert_eq!(present, expected);
            assert_eq!(range.happiness_adjustment_q16, 3 << 16);
            assert_eq!(range.departure_adjustment_q16, 1 << 16);
            assert!(watcher
                .sample(14.99, &entries, || panic!("sampled early"))
                .is_none());
        }
    }

    #[test]
    fn strict_upper_bound_and_moved_or_cleaned_cells_do_not_draw() {
        let mut watcher = watcher();
        let entries = [entry("bad", 0.5), entry("good", 0.2)];
        assert!(watcher
            .sample(0.0, &entries, || panic!("drew at range boundary"))
            .is_none());
        let dirty = [entry("bad", 0.5)];
        assert!(watcher.sample(5.0, &dirty, || 49).is_some());
        assert!(watcher
            .sample(10.0, &[], || panic!("drew in empty cell"))
            .is_none());
    }
}
