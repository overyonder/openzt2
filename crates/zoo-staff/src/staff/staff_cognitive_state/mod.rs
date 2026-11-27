use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    authored_float_attributes::AuthoredFloatAttributes, staff::staff_employment_types::StaffRole,
};

pub(super) fn project_staff_cognitive_eligibility(
    mut staff: Query<(
        Option<&AuthoredFloatAttributes>,
        Has<openzt2_interface_state::immersive_modes::immersive_mode_state_types::ControlledEntity>,
        Has<openzt2_zoo_state::staff::staff_employment_types::StaffClimbingTankGate>,
        &mut openzt2_zoo_state::staff::staff_job_types::StaffCognitiveEligibility,
    )>,
) {
    use openzt2_zoo_state::staff::staff_job_types::StaffCognitiveEligibility;
    let interruptible = AssetId::from_key("interruptible");
    let on_tap = AssetId::from_key("ontap");
    let suppress_update = AssetId::from_key("suppressupdate");
    for (facts, controlled, climbing, mut state) in &mut staff {
        let fact = |attribute| {
            facts
                .and_then(|facts| facts.get(attribute))
                .map(|value| value != 0.0)
        };
        state.set_if_neq(StaffCognitiveEligibility {
            interruptible: fact(interruptible).unwrap_or(true) && !climbing && !controlled,
            on_tap: fact(on_tap).unwrap_or(false),
            blocks_request_search: controlled || fact(suppress_update).unwrap_or(false),
        });
    }
}

#[derive(Clone, Copy)]
enum StaffNeedInitialValue {
    Constant(f32),
    Range { minimum: f32, maximum: f32 },
}

impl StaffNeedInitialValue {
    fn sample(self, draw: impl FnOnce() -> u16) -> f32 {
        match self {
            Self::Constant(value) => value,
            Self::Range { minimum, maximum } if maximum >= minimum => {
                let width = ((maximum - minimum) as u32).saturating_add(1);
                minimum + (u32::from(draw()) % width) as f32
            }
            Self::Range { minimum, .. } => minimum,
        }
    }
}

#[derive(Clone, Copy)]
struct StaffNeed {
    key: AssetId,
    value: f32,
    adjustment: f32,
    trigger_threshold: f32,
}

#[derive(Component)]
pub struct StaffNeeds(Vec<StaffNeed>);

impl StaffNeeds {
    pub(super) fn source_need(&self, attribute: AssetId) -> Option<(i32, bool)> {
        self.0
            .iter()
            .find(|variable| variable.key == attribute)
            .map(|variable| {
                (
                    (f64::from(variable.value) * 65_536.0).round() as i32,
                    variable.value >= variable.trigger_threshold,
                )
            })
    }
}

#[derive(Message)]
pub struct AdjustStaffNeed {
    pub staff: Entity,
    pub need: AssetId,
    pub delta: f32,
}

#[derive(Message)]
pub struct StaffNeedAdjustmentsDue {
    pub staff: Entity,
    pub count: u32,
}

pub(super) fn update_live_staff_needs(
    mut due: MessageReader<StaffNeedAdjustmentsDue>,
    mut adjustments: MessageReader<AdjustStaffNeed>,
    mut staff: Query<(&mut StaffNeeds, &mut AuthoredFloatAttributes)>,
    mut facts: MessageWriter<
        openzt2_staff_requests::staff_request_fact_changes::StaffRequestFactChanged,
    >,
) {
    use openzt2_staff_requests::staff_request_fact_changes::{
        StaffRequestFactChanged, StaffRequestFactValue,
    };

    let mut adjust =
        |owner, attributes: &mut Mut<AuthoredFloatAttributes>, need: &mut StaffNeed, delta: f32| {
            let previous = attributes.get(need.key).unwrap_or(need.value);
            let value = (previous + delta).clamp(0.0, 100.0);
            need.value = value;
            if value != previous {
                attributes.set(need.key, value);
                facts.write(StaffRequestFactChanged {
                    owner,
                    key: need.key,
                    previous: StaffRequestFactValue::Number(previous),
                    current: StaffRequestFactValue::Number(value),
                });
            }
        };
    for due in due.read() {
        if let Ok((mut needs, mut attributes)) = staff.get_mut(due.staff) {
            for need in &mut needs.bypass_change_detection().0 {
                let delta = need.adjustment * due.count as f32;
                adjust(due.staff, &mut attributes, need, delta);
            }
        }
    }
    for change in adjustments.read() {
        if let Ok((mut needs, mut attributes)) = staff.get_mut(change.staff) {
            if let Some(need) = needs
                .bypass_change_detection()
                .0
                .iter_mut()
                .find(|need| need.key == change.need)
            {
                adjust(change.staff, &mut attributes, need, change.delta);
            }
        }
    }
}

#[derive(Component, Default)]
pub struct StaffNeedAdjustmentTimer(Timer);

impl StaffNeedAdjustmentTimer {
    pub fn remaining(&self) -> std::time::Duration {
        self.0.remaining()
    }

    pub fn from_remaining(remaining: std::time::Duration) -> Self {
        Self(Timer::new(remaining, TimerMode::Once))
    }
}

pub(super) fn publish_staff_need_adjustment_due(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    time: Res<Time<Fixed>>,
    mut staff: Query<(
        Entity,
        &StaffNeeds,
        &mut StaffNeedAdjustmentTimer,
        &mut BehaviorRandomStream,
    )>,
    mut due: MessageWriter<StaffNeedAdjustmentsDue>,
) {
    let Some(policy) = active
        .get(&assets)
        .and_then(|definitions| definitions.need_adjustment())
        .filter(|policy| policy.enabled)
    else {
        return;
    };
    for (staff, needs, mut timer, mut random) in &mut staff {
        if needs.0.iter().all(|need| need.adjustment == 0.0) {
            continue;
        }
        if openzt2_zoo_state::need_adjustment::tick_need_adjustment(
            &mut timer.0,
            time.timestep(),
            policy.delay_steps,
            &mut random.0,
        ) {
            due.write(StaffNeedAdjustmentsDue { staff, count: 1 });
        }
    }
}

pub(super) fn initialize_staff_needs(
    mut commands: Commands,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut staff: Query<
        (Entity, &StaffRole, Option<&mut AuthoredFloatAttributes>),
        Without<StaffNeeds>,
    >,
) {
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    for (entity, role, attributes) in &mut staff {
        let Some(role) = definitions.find_staff(role.0) else {
            continue;
        };
        let mut initial = AuthoredFloatAttributes::default();
        let needs: Vec<StaffNeed> = role
            .state_variables
            .iter()
            .map(|row| StaffNeed {
                key: row.name,
                value: attributes
                    .as_deref()
                    .and_then(|values| values.get(row.name))
                    .unwrap_or_else(|| {
                        let initial = row.initial_range.map_or(
                            StaffNeedInitialValue::Constant(row.value),
                            |[minimum, maximum]| StaffNeedInitialValue::Range { minimum, maximum },
                        );
                        initial.sample(|| random.next_u15())
                    }),
                adjustment: row.adjustment.unwrap_or_default(),
                trigger_threshold: row.trigger_threshold,
            })
            .collect();
        if let Some(mut attributes) = attributes {
            for need in &needs {
                if attributes.get(need.key) != Some(need.value) {
                    attributes.set(need.key, need.value);
                }
            }
            commands.entity(entity).insert(StaffNeeds(needs));
        } else {
            for need in &needs {
                initial.set(need.key, need.value);
            }
            commands.entity(entity).insert((StaffNeeds(needs), initial));
        }
    }
}

#[derive(Component, Default)]
pub struct FossilStaffSensor {
    bad_sites: Vec<Entity>,
    last_scan: Option<f64>,
    countdown: f64,
    pub(super) teleport: bool,
}

impl FossilStaffSensor {
    pub fn saved_state(&self) -> (&[Entity], Option<f64>, f64, bool) {
        (
            &self.bad_sites,
            self.last_scan,
            self.countdown,
            self.teleport,
        )
    }

    pub fn from_saved_state(
        bad_sites: Vec<Entity>,
        last_scan: Option<f64>,
        countdown: f64,
        teleport: bool,
    ) -> Self {
        Self {
            bad_sites,
            last_scan,
            countdown,
            teleport,
        }
    }

    pub(super) fn scan(
        &mut self,
        now: f64,
        delay: f64,
        active_sites: &[Entity],
        is_live: impl Fn(Entity) -> bool,
    ) -> bool {
        let elapsed = self.last_scan.replace(now).map_or(0.0, |last| now - last);
        if self.countdown > 0.0 {
            self.countdown -= elapsed;
            if self.countdown > 0.0 {
                return false;
            }
            self.teleport = true;
            self.bad_sites.clear();
        }
        self.bad_sites.retain(|&site| is_live(site));
        if !active_sites.is_empty() && active_sites.iter().all(|&site| !self.accepts(site)) {
            self.countdown = delay;
            return false;
        }
        true
    }

    pub(super) fn accepts(&self, site: Entity) -> bool {
        !self.bad_sites.contains(&site)
    }

    pub(super) fn collect(&mut self, site: Entity) {
        if self.accepts(site) {
            self.bad_sites.push(site);
        }
        self.teleport = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_range_initialization_draws_once_in_row_order_and_constants_do_not_draw() {
        let mut draws = [30, 15, 0].into_iter();
        let rows = [
            StaffNeedInitialValue::Range {
                minimum: 20.0,
                maximum: 50.0,
            },
            StaffNeedInitialValue::Range {
                minimum: 10.0,
                maximum: 25.0,
            },
            StaffNeedInitialValue::Constant(7.0),
            StaffNeedInitialValue::Range {
                minimum: 20.0,
                maximum: 50.0,
            },
        ];
        assert_eq!(
            rows.map(|row| row.sample(|| draws.next().unwrap())),
            [50.0, 25.0, 7.0, 20.0]
        );
        assert_eq!(draws.next(), None);
    }

    #[test]
    fn exhausted_puzzle_sites_wait_until_the_deadline_then_enable_teleport() {
        let site = Entity::from_bits(7);
        let mut sensor = FossilStaffSensor::default();
        sensor.collect(site);
        assert!(!sensor.scan(30.0, 120.0, &[site], |_| true));
        assert!(!sensor.scan(149.99, 120.0, &[site], |_| true));
        assert!(!sensor.teleport);
        assert!(sensor.scan(150.0, 120.0, &[site], |_| true));
        assert!(sensor.teleport);
        assert!(sensor.accepts(site));
    }

    #[test]
    fn empty_sites_do_not_start_a_wait_and_dead_sites_are_pruned() {
        let retired = Entity::from_bits(7);
        let next = Entity::from_bits(8);
        let mut sensor = FossilStaffSensor::default();
        sensor.collect(retired);
        assert!(sensor.scan(0.0, 120.0, &[], |_| false));
        assert!(sensor.scan(1.0, 120.0, &[next], |site| site == next));
        assert!(sensor.bad_sites.is_empty());
        assert!(!sensor.teleport);
    }
}
