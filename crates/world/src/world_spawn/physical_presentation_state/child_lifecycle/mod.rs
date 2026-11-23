//! Completion notifications for authored timed child presentations.

use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationOperation;
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;

use super::{
    ObjectPhysicalPresentationControllerOf, PhysicalPresentationChildOf,
    PhysicalPresentationControllerDefinition,
};

/// References the canonical selected state; retains only its running clock.
#[derive(Component)]
pub(in crate::world_spawn) struct PhysicalChildPlayback {
    state: usize,
    started_tick: Option<u64>,
    completed_periods: u64,
}

impl PhysicalChildPlayback {
    pub(in crate::world_spawn) const fn new(state: usize) -> Self {
        Self {
            state,
            started_tick: None,
            completed_periods: 0,
        }
    }

    fn period_finished(&mut self, tick: u64, fixed_hz: u16, duration: std::time::Duration) -> bool {
        let started = *self.started_tick.get_or_insert(tick);
        let elapsed_units = u128::from(tick.saturating_sub(started)) * 1_000_000_000;
        let period_units = duration.as_nanos().saturating_mul(u128::from(fixed_hz));
        if duration.is_zero()
            || elapsed_units <= period_units.saturating_mul(u128::from(self.completed_periods) + 1)
        {
            return false;
        }
        self.completed_periods = self.completed_periods.saturating_add(1);
        true
    }
}

pub(in crate::world_spawn) fn advance_authored_child_lifetimes(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    controllers: Query<(
        &PhysicalPresentationControllerDefinition,
        &ObjectPhysicalPresentationControllerOf,
    )>,
    mut children: Query<
        (
            Entity,
            &PhysicalPresentationChildOf,
            &mut PhysicalChildPlayback,
        ),
        With<PrefabPresentationHydrated>,
    >,
    settings: Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>,
    mut notifications: MessageWriter<PhysicalPresentationRequest>,
) {
    let clock = clock.into_inner();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (child, controller, mut playback) in &mut children {
        let Ok((identity, owner)) = controllers.get(controller.0) else {
            continue;
        };
        let Some(definition) = definitions.find_object(identity.object).and_then(|object| {
            object
                .presentation_attachments
                .get(identity.controller_index)
        }) else {
            continue;
        };
        let Some(animation) = definition
            .states
            .get(playback.state)
            .and_then(|state| state.child_animation.as_ref())
        else {
            continue;
        };
        if !settings.physical_animations && !definition.overrides_animation_setting {
            continue;
        }
        if !animation.auto_start || animation.duration.is_zero() {
            continue;
        }
        // Native advances one period per update and uses a strict elapsed > duration test.
        if !playback.period_finished(
            clock.tick,
            definitions.timing().fixed_hz,
            animation.duration,
        ) {
            continue;
        }
        if !animation.looping {
            commands.entity(child).remove::<PhysicalChildPlayback>();
        }
        notifications.write(PhysicalPresentationRequest {
            owner: owner.0,
            operation: PhysicalPresentationOperation::ChildPeriodFinished {
                controller: controller.0,
                child,
                looping: animation.looping,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_periods_use_simulation_ticks_without_fractional_period_drift() {
        let mut playback = PhysicalChildPlayback::new(0);
        let period = std::time::Duration::from_millis(1320);
        assert!(!playback.period_finished(100, 30, period));
        assert!(!playback.period_finished(139, 30, period));
        assert!(playback.period_finished(140, 30, period));
        assert!(!playback.period_finished(140, 30, period));
        assert!(!playback.period_finished(179, 30, period));
        assert!(playback.period_finished(180, 30, period));
        assert!(playback.period_finished(219, 30, period));
    }
}
