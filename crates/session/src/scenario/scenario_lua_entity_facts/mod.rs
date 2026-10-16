use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::animal_lifecycle::types::{AnimalLifeStage, Pregnancy, SpeciesHandle};
use openzt2_zoo_state::animal_welfare::types::*;

/// Borrowed live components exposed to authored scenario entity queries.
#[derive(QueryData)]
pub(super) struct ScenarioLuaEntityFacts {
    pub(super) entity: Entity,
    pub(super) definition: &'static DefinitionId,
    pub(super) pregnant: Has<Pregnancy>,
    pub(super) crated:
        Has<openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated>,
    pub(super) stage: Option<&'static AnimalLifeStage>,
    pub(super) variant: Option<&'static openzt2_zoo_state::animal_lifecycle::types::AnimalVariant>,
    pub(super) species: Option<&'static SpeciesHandle>,
    pub(super) hunger: Option<&'static Hunger>,
    pub(super) thirst: Option<&'static Thirst>,
    pub(super) rest: Option<&'static RestNeed>,
    pub(super) privacy: Option<&'static PrivacyNeed>,
    pub(super) social: Option<&'static SocialNeed>,
    pub(super) exercise: Option<&'static ExerciseNeed>,
    pub(super) stimulation: Option<&'static StimulationNeed>,
    pub(super) environment: Option<&'static EnvironmentNeed>,
    pub(super) health: Option<&'static HealthNeed>,
    pub(super) hygiene: Option<&'static HygieneNeed>,
    pub(super) bathroom: Option<&'static BathroomNeed>,
}

impl ScenarioLuaEntityFactsItem<'_, '_> {
    pub(super) fn need_wellness_q16(&self, name: &str) -> Option<i32> {
        match name {
            "hunger" => self.hunger.map(|need| need.0),
            "thirst" => self.thirst.map(|need| need.0),
            "rest" => self.rest.map(|need| need.0),
            "privacy" => self.privacy.map(|need| need.0),
            "social" => self.social.map(|need| need.0),
            "exercise" => self.exercise.map(|need| need.0),
            "stimulation" => self.stimulation.map(|need| need.0),
            "environment" => self.environment.map(|need| need.0),
            "health" => self.health.map(|need| need.0),
            "hygiene" => self.hygiene.map(|need| need.0),
            "bathroom" => self.bathroom.map(|need| need.0),
            _ => None,
        }
    }
}
