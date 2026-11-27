//! Completion notifications for authored timed child presentations.

use bevy::{prelude::*, time::Stopwatch};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

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
    elapsed: Stopwatch,
}

impl PhysicalChildPlayback {
    pub(in crate::world_spawn) fn new(state: usize) -> Self {
        Self {
            state,
            elapsed: Stopwatch::new(),
        }
    }

    fn period_finished(&mut self, delta: std::time::Duration, period: std::time::Duration) -> bool {
        self.elapsed.tick(delta);
        if period.is_zero() || self.elapsed.elapsed() <= period {
            return false;
        }
        self.elapsed.set_elapsed(self.elapsed.elapsed() - period);
        true
    }
}

pub(in crate::world_spawn) fn advance_authored_child_lifetimes(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
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
        let delta = if playback.is_added() {
            std::time::Duration::ZERO
        } else {
            time.timestep()
        };
        if !playback.period_finished(delta, animation.duration) {
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
        let elapsed = |ticks: u64| std::time::Duration::from_nanos(33_333_333 * ticks);
        assert!(!playback.period_finished(elapsed(0), period));
        assert!(!playback.period_finished(elapsed(39), period));
        assert!(playback.period_finished(elapsed(1), period));
        assert!(!playback.period_finished(elapsed(0), period));
        assert!(!playback.period_finished(elapsed(39), period));
        assert!(playback.period_finished(elapsed(1), period));
        assert!(playback.period_finished(elapsed(39), period));
    }
}
