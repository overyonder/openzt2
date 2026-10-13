//! Revalidates authored consumption types before publishing a combined outcome.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::BehaviorCandidateRole,
    world_definitions::world_objects::WorldObjectContainerContent, AssetId,
};

use crate::animal_behavior::autonomous_task_selection::candidate_facts::AnimalCandidateFacts;
use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::candidate_requirements_are_satisfied;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

#[derive(SystemParam)]
pub(crate) struct AnimalConsumableCompatibility<'w, 's> {
    species_assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    species_index: Option<Res<'w, SpeciesAssets>>,
    world_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    world_definitions: Option<Res<'w, WorldDefinitions>>,
    animals: Query<'w, 's, AnimalCandidateFacts>,
    sources: Query<'w, 's, &'static DefinitionId, Without<WorldEntityIsCrated>>,
}

impl AnimalConsumableCompatibility<'_, '_> {
    pub(super) fn accepts(
        &self,
        animal: Entity,
        target: Entity,
        source: AssetId,
        content: WorldObjectContainerContent,
        task: &BehaviorTaskExecutionState,
        documents: &Assets<BehaviorDocumentAsset>,
    ) -> bool {
        let Ok(facts) = self.animals.get(animal) else {
            return false;
        };
        let Some(species) = self
            .species_index
            .as_ref()
            .and_then(|index| index.get(self.species_assets.as_deref()?))
        else {
            return false;
        };
        let Some((definition, variant)) = species.find_variant_with_species(facts.variant_id())
        else {
            return false;
        };
        if definition.id != facts.species.species
            || !self.sources.get(target).is_ok_and(|id| id.0 == source)
        {
            return false;
        }
        let Some(world) = self
            .world_definitions
            .as_ref()
            .and_then(|index| index.get(self.world_assets.as_deref()?))
        else {
            return false;
        };
        let Some(object) = world.find_object(source).filter(|object| {
            object
                .container_quantity
                .is_some_and(|quantity| quantity.content == content)
        }) else {
            return false;
        };
        // Nested sets keep the enclosing task's authored species/source contract.
        let origin = std::iter::once((&task.document, task.declaration, task.phase, task.program))
            .chain(task.stack.iter().rev().map(|frame| {
                (
                    &frame.document,
                    frame.declaration,
                    frame.phase,
                    frame.program,
                )
            }))
            .find(|(_, _, phase, _)| *phase != BehaviorTaskExecutionPhase::Set);
        let Some((document, declaration, _, program)) = origin else {
            return false;
        };
        let Some(authored_task) = documents
            .get(document)
            .and_then(|asset| asset.behavior_task_at_index(declaration))
            .filter(|authored_task| authored_task.id == program)
        else {
            return false;
        };
        let requirements = &authored_task.candidate_eligibility_requirements;
        if ![
            BehaviorCandidateRole::Subject,
            BehaviorCandidateRole::Target,
        ]
        .into_iter()
        .all(|role| {
            requirements.iter().any(|requirement| {
                requirement.candidate_role == role
                    && !requirement.candidate_type_identifiers.is_empty()
            })
        }) {
            return false;
        }
        candidate_requirements_are_satisfied(
            requirements,
            |role, required| match role {
                BehaviorCandidateRole::Subject => {
                    facts.matches_type(required, &variant.behavior_subject_type_identifiers)
                }
                BehaviorCandidateRole::Target => {
                    required == source || object.behavior_type_identifiers.contains(&required)
                }
                BehaviorCandidateRole::Object => false,
            },
            // Creation qualifiers include initial need thresholds. They are not
            // completion conditions; only the type contract is revalidated here.
            |_, _| true,
        )
    }
}
