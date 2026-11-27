mod animal_activity_immobilization_execution;
mod animal_escape_state_execution;
mod animal_health_staff_job_request_generation;
mod animal_rampage_execution;
pub use animal_rampage_execution::{RampageAudioMix, RampageAudioMixProjection};
mod animal_tranquilization_and_capture_execution;
mod disease_hint_ui_action_routing;
pub(crate) mod disease_mortality_execution;
mod disease_progression_calculations;
mod disease_progression_execution;
mod rampage_policy;
use openzt2_zoo_state::animal_health::tranquilizer_eligibility_calculation;
mod tranquilizer_fire_outcome_presentation;
mod tranquilizer_heads_up_display_projection;
use openzt2_zoo_state::animal_health::types;
mod veterinary_treatment_execution;

use bevy::{ecs::schedule::common_conditions::on_message, prelude::*};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;

use types::{
    AnimalCaptured, AnimalDied, CaptureAnimalRequest, FreezeAnimal, ThawAnimal, TranquilizeRequest,
    TranquilizerFired, TreatmentRequest, Vitality,
};

pub struct AnimalHealthPlugin;

impl Plugin for AnimalHealthPlugin {
    fn build(&self, app: &mut App) {
        rampage_policy::register_rampage_policy(app);
        animal_rampage_execution::register_rampage_timers(app);
        app.register_required_components_with::<Animal, Vitality>(|| Vitality(1.0));
        app.add_observer(animal_activity_immobilization_execution::stop_activity_when_frozen)
            .add_observer(animal_activity_immobilization_execution::cancel_tasks_started_for_inactive_animals)
            .add_observer(animal_tranquilization_and_capture_execution::discard_tranquilizer_dose_timer)
            .add_observer(animal_tranquilization_and_capture_execution::discard_tranquilizer_recovery_timer)
            .add_observer(veterinary_treatment_execution::cancel_removed_treatment)
            .add_message::<TreatmentRequest>()
            .add_message::<types::AnimalTreatmentSettled>()
            .add_message::<TranquilizeRequest>()
            .add_message::<CaptureAnimalRequest>()
            .add_message::<TranquilizerFired>()
            .add_message::<FreezeAnimal>()
            .add_message::<ThawAnimal>()
            .add_message::<AnimalDied>()
            .add_message::<AnimalCaptured>()
            .add_systems(
                Startup,
                tranquilizer_heads_up_display_projection::load_tranquilizer_charge_bar_images,
            )
            .add_systems(
                Update,
                disease_hint_ui_action_routing::
                    route_authored_increment_disease_hint_ui_actions_to_selected_animal_disease
                    .run_if(in_state(GamePhase::InGame))
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
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_activity_immobilization_execution::
                        apply_animal_freeze_and_thaw_requests,
                    (
                        veterinary_treatment_execution::
                            validate_veterinary_treatment_requests_and_begin_treatment
                            .run_if(on_message::<TreatmentRequest>),
                        disease_progression_execution::
                            advance_active_animal_diseases_and_apply_vitality_loss,
                        veterinary_treatment_execution::
                            advance_active_veterinary_treatments_and_apply_completed_results
                            .after(veterinary_treatment_execution::
                                validate_veterinary_treatment_requests_and_begin_treatment)
                            .after(disease_progression_execution::
                                advance_active_animal_diseases_and_apply_vitality_loss),
                    ),
                    (
                        animal_tranquilization_and_capture_execution::
                            validate_tranquilizer_requests_and_apply_timed_tranquilization
                            .run_if(on_message::<TranquilizeRequest>),
                        animal_tranquilization_and_capture_execution::
                            validate_staff_capture_requests_and_clear_animal_emergency_state,
                        animal_tranquilization_and_capture_execution::
                            advance_animal_tranquilization_and_begin_recovery_period,
                        animal_tranquilization_and_capture_execution::
                            advance_and_complete_animal_tranquilizer_recovery_periods,
                    )
                        .chain(),
                )
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    animal_escape_state_execution::clear_animal_escape_state_after_crating,
                    animal_escape_state_execution::
                        clear_animal_escape_state_after_player_relocation,
                    animal_escape_state_execution::
                        detect_animal_containment_breaches_and_mark_newly_escaped_animals
                        .after(animal_escape_state_execution::clear_animal_escape_state_after_crating)
                        .after(animal_escape_state_execution::clear_animal_escape_state_after_player_relocation),
                    animal_health_staff_job_request_generation::
                        request_tranquilization_for_rampaging_animals,
                    animal_escape_state_execution::
                        clear_escape_and_rampage_after_tranquilized_animal_reaches_containment
                        .after(animal_escape_state_execution::
                            detect_animal_containment_breaches_and_mark_newly_escaped_animals)
                        .after(animal_health_staff_job_request_generation::
                            request_tranquilization_for_rampaging_animals),
                    disease_mortality_execution::
                        finalize_disease_deaths_after_vitality_reaches_zero
                        .after(animal_escape_state_execution::
                            clear_escape_and_rampage_after_tranquilized_animal_reaches_containment),
                )
                    .in_set(FixedGameSet::Cleanup),
            );
    }
}
