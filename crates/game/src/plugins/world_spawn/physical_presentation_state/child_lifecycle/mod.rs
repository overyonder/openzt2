//! Completion notifications for authored timed child presentations.

use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

use super::super::{
    prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection,
    prefab_presentation_types::PrefabPresentationHydrated,
};
use super::{PhysicalPresentationOperation, PhysicalPresentationRequest};

/// References the canonical selected state; retains only its running clock.
#[derive(Component)]
pub(in crate::plugins::world_spawn) struct PhysicalChildPlayback {
    pub(in crate::plugins::world_spawn) owner: Entity,
    pub(in crate::plugins::world_spawn) controller: usize,
    pub(in crate::plugins::world_spawn) state: usize,
    pub(in crate::plugins::world_spawn) started_tick: Option<u64>,
    pub(in crate::plugins::world_spawn) completed_periods: u64,
}

impl PhysicalChildPlayback {
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

pub(in crate::plugins::world_spawn) fn advance_authored_child_lifetimes(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    owners: Query<&PrefabObjectPresentationAttachmentProjection>,
    mut children: Query<(Entity, &mut PhysicalChildPlayback), With<PrefabPresentationHydrated>>,
    settings: Res<crate::plugins::settings::graphics_settings_types::GraphicsSettings>,
    mut notifications: MessageWriter<PhysicalPresentationRequest>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (child, mut playback) in &mut children {
        let Some(controller) = owners
            .get(playback.owner)
            .ok()
            .and_then(|identity| definitions.find_object(identity.0))
            .and_then(|object| object.presentation_attachments.get(playback.controller))
        else {
            continue;
        };
        let Some(animation) = controller
            .states
            .get(playback.state)
            .and_then(|state| state.child_animation.as_ref())
        else {
            continue;
        };
        if !settings.physical_animations && !controller.overrides_animation_setting {
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
        let operation = if animation.looping {
            PhysicalPresentationOperation::Looped(child)
        } else {
            commands.entity(child).remove::<PhysicalChildPlayback>();
            PhysicalPresentationOperation::Completed(child)
        };
        notifications.write(PhysicalPresentationRequest {
            owner: playback.owner,
            operation,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_periods_use_simulation_ticks_without_fractional_period_drift() {
        let mut world = World::new();
        let mut playback = PhysicalChildPlayback {
            owner: world.spawn_empty().id(),
            controller: 0,
            state: 0,
            started_tick: None,
            completed_periods: 0,
        };
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
