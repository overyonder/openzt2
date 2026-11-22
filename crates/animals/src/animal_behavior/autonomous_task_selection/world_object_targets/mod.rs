//! Placed world objects as concrete targets of authored animal tasks.

use super::candidate_facts::compare_q16;
use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
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
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

/// World object candidates and the location queries their qualifiers use.
#[derive(SystemParam)]
pub(crate) struct WorldObjectTargetSources<'w, 's> {
    pub(super) objects:
        Query<'w, 's, WorldObjectTargetFacts, (Without<Animal>, Without<WorldEntityIsCrated>)>,
    roots: Query<'w, 's, (&'static HabitatIndex, &'static TopologyGrid)>,
    pub(super) terrain_surfaces: DrySurfaceEligibility<'w, 's>,
    /// Fences an animal bashes, jumps, climbs or tunnels under.
    pub(super) fences: Query<
        'w,
        's,
        (
            openzt2_zoo_state::topology::topology_graph_types::FenceSegment,
            &'static GlobalTransform,
        ),
    >,
    pub(super) habitat_summaries:
        Query<'w, 's, &'static openzt2_zoo_state::habitat::habitat_types::HabitatSummary>,
    pub(super) topology_nodes:
        Query<'w, 's, &'static openzt2_zoo_state::topology::topology_graph_types::TopologyNode>,
    pub(super) habitat_boundaries: HabitatBoundaryQueries<'w, 's>,
}

impl WorldObjectTargetSources<'_, '_> {
    /// The middle of a fence segment, whose origin is its first end, on the
    /// ground there.
    pub(super) fn fence_middle(
        &self,
        root: Entity,
        edge: &openzt2_zoo_state::topology::topology_graph_types::FenceSegmentItem<'_, '_>,
    ) -> Option<Vec3> {
        let (_, grid) = self.roots.get(root).ok()?;
        let first = grid.cell_translation(self.topology_nodes.get(edge.a()).ok()?.cell);
        let second = grid.cell_translation(self.topology_nodes.get(edge.b()).ok()?.cell);
        // On the ground, not at the topology layer's height.
        let middle = (first + second) * 0.5;
        Some(
            middle.with_y(
                self.terrain_surfaces
                    .ground_height(root, middle.xz())
                    .unwrap_or(middle.y),
            ),
        )
    }

    /// Whether an escaped animal can reach a world position.
    pub(super) fn escaped_animal_reaches(&self, root: Entity, position: Vec3) -> bool {
        let Ok((index, grid)) = self.roots.get(root) else {
            return false;
        };
        super::super::escaped_animal_reaches(index, *grid, &self.habitat_summaries, position.xz())
    }

    /// The fenced habitat containing a world position, if any.
    pub(super) fn habitat_at(&self, root: Entity, position: Vec3) -> Option<Entity> {
        let (index, grid) = self.roots.get(root).ok()?;
        locate_habitat_at_world_position(index, Vec2::new(position.x, position.z), *grid)
    }
}

#[derive(QueryData)]
pub(crate) struct WorldObjectTargetFacts {
    pub(super) entity: Entity,
    pub(super) definition: &'static DefinitionId,
    pub(super) transform: &'static GlobalTransform,
    pub(super) world: &'static WorldMember,
    food: Option<&'static FoodContainer>,
    drink: Option<&'static DrinkContainer>,
    attacked: Has<openzt2_zoo_state::guests::guest_simulation_types::GuestAttacked>,
}

/// Location facts about one target, read only when a qualifier needs them.
pub(super) struct WorldObjectTargetLocation<OnLand: Fn() -> Option<bool>> {
    /// The target stands in the habitat that restricts the animal.
    pub(super) in_habitat: bool,
    pub(super) on_land: OnLand,
    /// How far the target is from the animal, for `inSight`.
    pub(super) distance: f32,
    /// The target's `f_BuildingStrength`, if it can be destroyed.
    pub(super) building_strength: Option<u16>,
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
            ) => truth((location.on_land)().is_some()),
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
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::Attacked) => {
                truth(self.attacked)
            }
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InSight) => {
                return location.distance < expected as f32 / 65_536.0;
            }
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::BuildingStrength,
            ) => {
                let Some(strength) = location.building_strength else {
                    return false;
                };
                i32::from(strength) << 16
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
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::FoodLevel
                    | BehaviorConditionEligibilityFact::BuildingStrength
            ) | BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface
                    | BehaviorSpatialEligibilityFact::OnLand
                    | BehaviorSpatialEligibilityFact::InWater
                    | BehaviorSpatialEligibilityFact::InSight
            ) | BehaviorEligibilityFactInput::State(
                BehaviorEntityStateEligibilityFact::InHabitat
                    | BehaviorEntityStateEligibilityFact::Attacked
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
