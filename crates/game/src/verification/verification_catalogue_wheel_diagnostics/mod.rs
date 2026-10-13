use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::ui::picking::UiWheelCapture;
use openzt2_game_assets::application_lifecycle::GamePhase;

use super::{
    verification_catalogue_wheel_input::{
        CatalogueWheelInputPlan, VerificationCatalogueWheelInput,
    },
    verification_journey_state_types::{VerificationJourneySession, VerificationSessionStage},
    verification_pointer_reach::VerificationPointerReach,
};

#[derive(Resource, Default)]
pub(crate) struct CatalogueWheelDiagnostic {
    current: Option<WheelStepObservation>,
}

struct WheelStepObservation {
    journey_index: usize,
    step_index: usize,
    world_loads_started: u32,
    world: Entity,
    definition: AssetId,
    row: Entity,
    list: Entity,
    emission_frame: u64,
    consumer_recorded: bool,
    layout_samples: u8,
}

pub(super) fn observes_remembered_catalogue_journey(session: &VerificationJourneySession) -> bool {
    !session.finished
        && session.stage == VerificationSessionStage::Journey
        && session.observed_phase == GamePhase::InGame
        && session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "remembered-catalogue-selection")
}

impl CatalogueWheelDiagnostic {
    pub(super) fn retire_changed_step(
        &mut self,
        session: &VerificationJourneySession,
        world: Option<Entity>,
    ) {
        if !observes_remembered_catalogue_journey(session)
            || self.current.as_ref().is_some_and(|current| {
                current.journey_index != session.journey_index
                    || current.step_index != session.step_index
                    || current.world_loads_started != session.world_loads_started
                    || Some(current.world) != world
            })
        {
            self.current = None;
        }
    }

    pub(super) fn observe_emission(
        &mut self,
        session: &VerificationJourneySession,
        definition: AssetId,
        plan: CatalogueWheelInputPlan,
        world: Option<Entity>,
    ) {
        let Some(world) = world else {
            self.current = None;
            info!(target: "openzt2_verification", "catalogue wheel diagnostic requires one current world");
            return;
        };
        self.current = Some(WheelStepObservation {
            journey_index: session.journey_index,
            step_index: session.step_index,
            world_loads_started: session.world_loads_started,
            world,
            definition,
            row: plan.row,
            list: plan.list,
            emission_frame: session.frames_run,
            consumer_recorded: false,
            layout_samples: 0,
        });
    }

    pub(crate) fn awaiting_consumer(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|current| !current.consumer_recorded)
    }

    pub(crate) fn record_consumer(
        &mut self,
        receiver: UiWheelCapture,
        delta: f32,
        applied: Option<(Vec2, Vec2, Vec2, OverflowAxis, OverflowAxis)>,
        outcome: &str,
    ) {
        let Some(current) = self
            .current
            .as_mut()
            .filter(|current| !current.consumer_recorded)
        else {
            return;
        };
        current.consumer_recorded = true;
        info!(target: "openzt2_verification", frame = current.emission_frame,
            step = current.step_index, world = ?current.world, definition = ?current.definition,
            row = ?current.row, planned_list = ?current.list, ?receiver, delta, outcome,
            before = ?applied.map(|value| value.0), after = ?applied.map(|value| value.1),
            maximum = ?applied.map(|value| value.2),
            axis_x = ?applied.map(|value| value.3), axis_y = ?applied.map(|value| value.4),
            "catalogue wheel actual consumer");
    }
}

pub(super) fn register_catalogue_wheel_diagnostics(app: &mut App) {
    app.init_resource::<CatalogueWheelDiagnostic>().add_systems(
        Last,
        observe_catalogue_wheel_after_layout_and_selection_reveal,
    );
}

fn observe_catalogue_wheel_after_layout_and_selection_reveal(
    session: Res<VerificationJourneySession>,
    mut diagnostic: ResMut<CatalogueWheelDiagnostic>,
    catalogue: VerificationCatalogueWheelInput,
    phase: Res<State<GamePhase>>,
    reach: VerificationPointerReach,
) {
    if *phase.get() != GamePhase::InGame {
        diagnostic.current = None;
        return;
    }
    diagnostic.retire_changed_step(&session, catalogue.current_world());
    let Some(current) = diagnostic.current.as_mut() else {
        return;
    };
    if current.layout_samples >= 2 {
        return;
    }
    // Last follows both Bevy layout and the PostUpdate focus/selection reveal.
    // Only the emitted frame and its next frame are sampled, not the whole wait.
    current.layout_samples += 1;
    if !current.consumer_recorded {
        info!(target: "openzt2_verification", frame = session.frames_run,
            step = current.step_index, list = ?current.list,
            "catalogue wheel has no consumer observation after Update");
    }
    catalogue.log_catalogue_wheel_row_boundary(
        current.row,
        current.definition,
        current.list,
        &reach,
        "after-layout-and-selection-reveal",
        session.frames_run,
        None,
    );
}
