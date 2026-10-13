//! Placed world objects as concrete targets of authored animal tasks.

use super::candidate_facts::compare_q16;
use crate::plugins::{
    animal_lifecycle::types::Animal,
    feeding::container_quantity::{DrinkContainer, FoodContainer},
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::HabitatIndex,
    },
    terrain::dry_surface_eligibility::TerrainDrySurfaceEligibility,
    topology::topology_graph_types::TopologyGrid,
    world_spawn::{
        world_entity_crating::WorldEntityIsCrated,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
        BehaviorCandidateRole, BehaviorConditionEligibilityFact, BehaviorEligibilityFactInput,
        BehaviorEligibilityFactJunction, BehaviorEligibilityFactValue,
        BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
    },
    world_definitions::world_objects::{WorldObjectContainerContent, WorldObjectDefinition},
    AssetId,
};

/// World object candidates and the location queries their qualifiers use.
#[derive(SystemParam)]
pub(in crate::plugins::animal_behavior) struct WorldObjectTargetSources<'w, 's> {
    pub(super) objects:
        Query<'w, 's, WorldObjectTargetFacts, (Without<Animal>, Without<WorldEntityIsCrated>)>,
    pub(super) indexed_objects: Query<'w, 's, (Entity, &'static DefinitionId), Without<Animal>>,
    pub(super) added_objects: Query<'w, 's, (), (Added<DefinitionId>, Without<Animal>)>,
    pub(super) removed_objects: RemovedComponents<'w, 's, DefinitionId>,
    pub(super) index: Local<'s, WorldObjectTargetTypeIndex>,
    pub(super) candidates: Local<'s, Vec<Entity>>,
    pub(super) habitat_index: Res<'w, HabitatIndex>,
    pub(super) topology_grid: Res<'w, TopologyGrid>,
    pub(super) terrain_surfaces: TerrainDrySurfaceEligibility<'w, 's>,
}

impl WorldObjectTargetSources<'_, '_> {
    /// Marks the type index stale when objects appeared or disappeared. Runs
    /// every update so removals don't accumulate while no animal is idle.
    pub(super) fn note_object_changes(&mut self) {
        if self.removed_objects.read().count() > 0 || !self.added_objects.is_empty() {
            self.index.mark_stale();
        }
    }

    pub(super) fn refresh_index(&mut self, definitions: WorldDefinitionsView<'_>) {
        self.index.rebuild_if_stale(
            definitions,
            self.indexed_objects
                .iter()
                .map(|(entity, definition)| (entity, definition.0)),
        );
    }

    /// The fenced habitat containing a world position, if any.
    pub(super) fn habitat_at(&self, position: Vec3) -> Option<Entity> {
        locate_habitat_at_world_position(
            &self.habitat_index,
            Vec2::new(position.x, position.z),
            *self.topology_grid,
        )
    }
}

#[derive(QueryData)]
pub(in crate::plugins::animal_behavior) struct WorldObjectTargetFacts {
    pub(super) entity: Entity,
    pub(super) definition: &'static DefinitionId,
    pub(super) transform: &'static GlobalTransform,
    pub(super) world: &'static WorldMember,
    food: Option<&'static FoodContainer>,
    drink: Option<&'static DrinkContainer>,
}

/// Location facts about one target, read only when a qualifier needs them.
pub(super) struct WorldObjectTargetLocation<OnLand: Fn() -> Option<bool>> {
    /// The target stands in the habitat that restricts the animal.
    pub(super) in_habitat: bool,
    pub(super) on_land: OnLand,
}

impl WorldObjectTargetFactsItem<'_, '_> {
    /// A container definition is a target only once its quantity matches the
    /// authored content; other objects never answer quantity qualifiers.
    pub(super) fn has_consistent_container_state(&self, object: &WorldObjectDefinition) -> bool {
        let Some(quantity) = object.container_quantity else {
            return self.food.is_none() && self.drink.is_none();
        };
        self.source_quantity().is_some()
            && match quantity.content {
                WorldObjectContainerContent::Food => self.food.is_some() && self.drink.is_none(),
                WorldObjectContainerContent::Drink => self.drink.is_some() && self.food.is_none(),
            }
    }

    pub(super) fn matches_type(&self, required: AssetId, object: &WorldObjectDefinition) -> bool {
        self.definition.0 == required || object.behavior_type_identifiers.contains(&required)
    }

    fn source_quantity(&self) -> Option<i32> {
        self.food
            .filter(|food| food.food == self.definition.0)
            .map(|food| food.amount_q16)
            .or_else(|| {
                self.drink
                    .filter(|drink| drink.drink == self.definition.0)
                    .map(|drink| drink.amount_q16)
            })
    }

    pub(super) fn satisfies(
        &self,
        fact: &BehaviorCandidateEligibilityFact,
        location: &WorldObjectTargetLocation<impl Fn() -> Option<bool>>,
    ) -> bool {
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let truth = |value: bool| i32::from(value) << 16;
        let value = match fact.fact_input {
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::FoodLevel,
            ) => {
                let Some(quantity) = self.source_quantity() else {
                    return false;
                };
                quantity
            }
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface,
            ) => truth(true),
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::OnLand) => {
                let Some(on_land) = (location.on_land)() else {
                    return false;
                };
                truth(on_land)
            }
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater) => {
                let Some(on_land) = (location.on_land)() else {
                    return false;
                };
                truth(!on_land)
            }
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InHabitat) => {
                truth(location.in_habitat)
            }
            _ => return false,
        };
        compare_q16(value, expected, fact.comparison)
    }
}

fn can_evaluate_world_object_fact(fact: &BehaviorCandidateEligibilityFact) -> bool {
    matches!(fact.expected_value, BehaviorEligibilityFactValue::Q16(_))
        && matches!(
            fact.fact_input,
            BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::FoodLevel)
                | BehaviorEligibilityFactInput::Spatial(
                    BehaviorSpatialEligibilityFact::SupportedSurface
                        | BehaviorSpatialEligibilityFact::OnLand
                        | BehaviorSpatialEligibilityFact::InWater
                )
                | BehaviorEligibilityFactInput::State(
                    BehaviorEntityStateEligibilityFact::InHabitat
                )
        )
}

/// The authored target requirement used to look up candidates, when every
/// target qualifier junction can be decided from world object facts.
pub(super) fn indexed_world_object_target_requirement(
    requirements: &[BehaviorCandidateEligibilityRequirements],
) -> Option<&BehaviorCandidateEligibilityRequirements> {
    let mut indexed = None;
    for requirement in requirements {
        if requirement.candidate_role != BehaviorCandidateRole::Target {
            continue;
        }
        let supported = requirement.eligibility_facts.is_empty()
            || match requirement.eligibility_fact_junction {
                BehaviorEligibilityFactJunction::All => requirement
                    .eligibility_facts
                    .iter()
                    .all(can_evaluate_world_object_fact),
                BehaviorEligibilityFactJunction::Any => requirement
                    .eligibility_facts
                    .iter()
                    .any(can_evaluate_world_object_fact),
            };
        if !supported {
            return None;
        }
        if indexed.is_none() && !requirement.candidate_type_identifiers.is_empty() {
            indexed = Some(requirement);
        }
    }
    indexed
}

/// Placed objects by every behavior type they answer to, rebuilt when objects
/// appear or disappear. Live facts are read from the canonical components.
#[derive(Default)]
pub(super) struct WorldObjectTargetTypeIndex {
    targets_by_type: HashMap<AssetId, Vec<Entity>>,
    current: bool,
}

impl WorldObjectTargetTypeIndex {
    /// Objects appeared or disappeared since the index was built.
    pub(super) const fn mark_stale(&mut self) {
        self.current = false;
    }

    pub(super) fn rebuild_if_stale(
        &mut self,
        definitions: WorldDefinitionsView<'_>,
        objects: impl Iterator<Item = (Entity, AssetId)>,
    ) {
        if self.current {
            return;
        }
        self.targets_by_type.clear();
        for (entity, definition) in objects {
            let Some(object) = definitions.find_object(definition) else {
                continue;
            };
            for type_identifier in std::iter::once(&definition).chain(
                object
                    .behavior_type_identifiers
                    .iter()
                    .filter(|type_identifier| **type_identifier != definition),
            ) {
                self.targets_by_type
                    .entry(*type_identifier)
                    .or_default()
                    .push(entity);
            }
        }
        // Objects whose definitions are still loading join on a later rebuild.
        self.current = !definitions.has_pending_assets();
    }

    /// Candidate objects for one target requirement, in a stable order and
    /// without duplicates. Every candidate must still pass the full check.
    pub(super) fn collect_candidates(
        &self,
        requirement: &BehaviorCandidateEligibilityRequirements,
        candidates: &mut Vec<Entity>,
    ) {
        candidates.clear();
        match requirement.candidate_type_junction {
            // Every candidate answers to the first type, among the others.
            BehaviorEligibilityFactJunction::All => {
                if let Some(first) = requirement.candidate_type_identifiers.first() {
                    candidates.extend(self.targets_by_type.get(first).into_iter().flatten());
                }
            }
            BehaviorEligibilityFactJunction::Any => {
                for type_identifier in &requirement.candidate_type_identifiers {
                    candidates.extend(
                        self.targets_by_type
                            .get(type_identifier)
                            .into_iter()
                            .flatten(),
                    );
                }
                candidates.sort_unstable();
                candidates.dedup();
            }
        }
    }
}
