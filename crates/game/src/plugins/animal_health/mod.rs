mod animal_activity_immobilization_execution;
mod animal_escape_state_execution;
mod animal_health_staff_job_request_generation;
mod animal_health_state_initialization;
mod animal_rampage_execution;
mod animal_tranquilization_and_capture_execution;
mod disease_hint_ui_action_routing;
mod disease_mortality_execution;
mod disease_progression_calculations;
mod disease_progression_execution;
pub(crate) mod tranquilizer_eligibility_calculation;
mod tranquilizer_fire_outcome_presentation;
mod tranquilizer_heads_up_display_projection;
mod tranquilizer_tool_intent_execution;
pub(crate) mod types;
mod veterinary_treatment_execution;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};

use types::{
    AimTranquilizer, AnimalCaptured, AnimalDied, CaptureAnimalRequest, FireTranquilizer,
    FreezeAnimal, ThawAnimal, TranquilizeRequest, TranquilizerFired, TreatmentRequest,
};

pub(crate) struct AnimalHealthPlugin;

impl Plugin for AnimalHealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TreatmentRequest>()
            .add_message::<TranquilizeRequest>()
            .add_message::<CaptureAnimalRequest>()
            .add_message::<AimTranquilizer>()
            .add_message::<FireTranquilizer>()
            .add_message::<TranquilizerFired>()
            .add_message::<FreezeAnimal>()
            .add_message::<ThawAnimal>()
            .add_message::<AnimalDied>()
            .add_message::<AnimalCaptured>()
            .add_systems(
                Update,
                (
                    (
                        tranquilizer_tool_intent_execution::
                            project_tranquilizer_target_distance_charge_and_reticle_state,
                        tranquilizer_tool_intent_execution::
                            resolve_tranquilizer_fire_requests_to_shot_or_misfire_outcomes,
                    )
                        .chain(),
                    disease_hint_ui_action_routing::
                        route_authored_increment_disease_hint_ui_actions_to_selected_animal_disease
                        .run_if(in_state(GamePhase::InGame)),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    tranquilizer_fire_outcome_presentation::
                        present_tranquilizer_fire_audio_and_particle_effects,
                    tranquilizer_heads_up_display_projection::
                        project_tranquilizer_heads_up_display_to_authored_ui_nodes,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_health_state_initialization::
                        initialize_new_animals_with_full_vitality,
                    animal_rampage_execution::evaluate_animal_rampage_start_and_end_conditions,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_activity_immobilization_execution::
                        apply_animal_freeze_and_thaw_requests,
                    animal_activity_immobilization_execution::
                        remove_new_activity_state_from_frozen_animals,
                    veterinary_treatment_execution::
                        validate_veterinary_treatment_requests_and_begin_treatment,
                    disease_progression_execution::
                        advance_active_animal_diseases_and_apply_vitality_loss,
                    veterinary_treatment_execution::
                        advance_active_veterinary_treatments_and_apply_completed_results,
                    animal_tranquilization_and_capture_execution::
                        validate_tranquilizer_requests_and_apply_timed_tranquilization,
                    animal_tranquilization_and_capture_execution::
                        validate_staff_capture_requests_and_clear_animal_emergency_state,
                    animal_tranquilization_and_capture_execution::
                        advance_animal_tranquilization_and_begin_recovery_period,
                    animal_tranquilization_and_capture_execution::
                        advance_and_complete_animal_tranquilizer_recovery_periods,
                    animal_rampage_execution::
                        advance_elapsed_ticks_for_active_untranquilized_animal_rampages,
                )
                    .chain()
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_escape_state_execution::clear_animal_escape_state_after_crating,
                    animal_escape_state_execution::
                        clear_animal_escape_state_after_player_relocation,
                    animal_escape_state_execution::
                        detect_animal_containment_breaches_and_mark_newly_escaped_animals,
                    animal_health_staff_job_request_generation::
                        request_staff_jobs_for_diseased_rampaging_escaped_and_tranquilized_animals,
                    animal_escape_state_execution::
                        clear_escape_and_rampage_after_tranquilized_animal_reaches_containment,
                    disease_mortality_execution::
                        finalize_disease_deaths_after_vitality_reaches_zero,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}
