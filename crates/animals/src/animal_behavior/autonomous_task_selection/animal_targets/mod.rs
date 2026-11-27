//! Other animals as task targets, such as the male an aardvark female invites
//! to mate, and the inviting female a `fromToken` task answers.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::behavior::eligibility::{
    BehaviorCandidateEligibilityFact, BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
    BehaviorEntityStateEligibilityFact, BehaviorRelationshipEligibilityFact,
    BehaviorSpatialEligibilityFact,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity;
use openzt2_zoo_state::animal_behavior::animal_lineage::AnimalLineage;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, Parents};
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;

use super::candidate_facts::{compare_q16, AnimalCandidateFacts, AnimalCandidateFactsItem};

#[derive(SystemParam)]
pub(crate) struct AnimalTargetSources<'w, 's> {
    pub(super) animals:
        Query<'w, 's, (Entity, AnimalCandidateFacts), (With<Animal>, UncratedWorldEntity)>,
    lineage: AnimalLineage<'w, 's>,
    surfaces: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    water_quality:
        Query<'w, 's, &'static openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality>,
    shores: crate::animal_behavior::natural_water_targets::NaturalWaterTargets<'w, 's>,
    biome: openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorTargetBiome<
        'w,
        's,
    >,
}

impl AnimalTargetSources<'_, '_> {
    pub(super) fn spatial_satisfies(
        &self,
        animal: &AnimalCandidateFactsItem,
        fact: &BehaviorCandidateEligibilityFact,
        day_fraction: f64,
    ) -> Option<bool> {
        use openzt2_game_data::behavior::eligibility::BehaviorEligibilityFactComparison as Comparison;
        let root = animal.world.root;
        let pose = animal.transform.translation();
        if let (
            BehaviorEligibilityFactInput::Taxonomy(
                openzt2_game_data::behavior::eligibility::BehaviorTaxonomyEligibilityFact::Biome,
            ),
            BehaviorEligibilityFactValue::Symbol(biome),
        ) = (fact.fact_input, fact.expected_value)
        {
            let matches = self.biome.at(root, pose) == Some(biome);
            return Some(if fact.comparison == Comparison::NotEqual {
                !matches
            } else {
                matches
            });
        }
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return None;
        };
        let mut comparison = fact.comparison;
        let source_value = match fact.fact_input {
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::DepthAboveBottom,
            ) => pose.y - self.surfaces.ground_height(root, pose.xz())?,
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::DepthBelowSurface,
            ) => self.surfaces.water_surface_height(root, pose.xz())? - pose.y,
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::ShoreDistance,
            ) => self.shore_distance_for_threshold(root, pose, expected),
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::WaterDirtiness,
            ) => {
                let quality = self
                    .water_quality
                    .get(animal.habitat_entity()?)
                    .ok()?
                    .0
                    .min(1000);
                f32::from(1000 - quality) / 1000.0
            }
            BehaviorEligibilityFactInput::Spatial(
                kind @ (BehaviorSpatialEligibilityFact::TimeOfDay
                | BehaviorSpatialEligibilityFact::TimeOfDayMin
                | BehaviorSpatialEligibilityFact::TimeOfDayMax),
            ) => {
                if comparison == Comparison::Equal {
                    comparison = match kind {
                        BehaviorSpatialEligibilityFact::TimeOfDayMin => Comparison::GreaterOrEqual,
                        BehaviorSpatialEligibilityFact::TimeOfDayMax => Comparison::LessOrEqual,
                        _ => Comparison::Equal,
                    };
                }
                (day_fraction * 24.0) as f32
            }
            _ => return None,
        };
        Some(compare_q16(
            (source_value * Q16_ONE as f32).round() as i32,
            expected,
            comparison,
        ))
    }

    pub(super) fn shore_distance_for_threshold(
        &self,
        root: Entity,
        pose: Vec3,
        expected: i32,
    ) -> f32 {
        let threshold = expected as f32 / Q16_ONE as f32;
        let tolerance = 1.0 / Q16_ONE as f32;
        self.shores
            .distance_to_shore_within(root, pose.xz(), threshold.max(0.0) + tolerance)
            .unwrap_or(threshold.max(0.0) + 2.0 * tolerance)
    }

    pub(super) fn spatial_value_at(
        &self,
        root: Entity,
        pose: Vec3,
        kind: BehaviorSpatialEligibilityFact,
        expected: i32,
    ) -> Option<f32> {
        Some(match kind {
            BehaviorSpatialEligibilityFact::DepthAboveBottom => {
                pose.y - self.surfaces.ground_height(root, pose.xz())?
            }
            BehaviorSpatialEligibilityFact::DepthBelowSurface => {
                self.surfaces.water_surface_height(root, pose.xz())? - pose.y
            }
            BehaviorSpatialEligibilityFact::WaterDepth => {
                self.surfaces.water_surface_height(root, pose.xz())?
                    - self.surfaces.ground_height(root, pose.xz())?
            }
            BehaviorSpatialEligibilityFact::ShoreDistance => {
                self.shore_distance_for_threshold(root, pose, expected)
            }
            _ => return None,
        })
    }

    /// `canMate`: the target is an adult that isn't the subject's parent, child
    /// or sibling.
    fn can_mate(
        &self,
        subject: Entity,
        target: Entity,
        target_facts: &AnimalCandidateFactsItem,
    ) -> bool {
        let (subject_parents, target_parents) =
            (self.lineage.parents(subject), self.lineage.parents(target));
        let parent_of = |parents: Parents, animal| {
            parents.mother == Some(animal) || parents.father == Some(animal)
        };
        let siblings = |a: Option<Entity>, b: Option<Entity>| a.is_some() && a == b;
        !target_facts.is_dead()
            && target_facts.is_adult()
            && !parent_of(subject_parents, target)
            && !parent_of(target_parents, subject)
            && !siblings(subject_parents.mother, target_parents.mother)
            && !siblings(subject_parents.father, target_parents.father)
    }

    /// Evaluates one authored target fact on another animal, relative to the
    /// subject where the fact is a relationship or a place.
    pub(super) fn target_satisfies(
        &self,
        subject: Entity,
        subject_facts: &AnimalCandidateFactsItem,
        target: Entity,
        target_facts: &AnimalCandidateFactsItem,
        fact: &BehaviorCandidateEligibilityFact,
        tick: u64,
        ticks_per_day: f64,
        day_fraction: f64,
        target_types: &[AssetId],
    ) -> bool {
        if let BehaviorEligibilityFactInput::Relationship(relationship) = fact.fact_input {
            if relationship == BehaviorRelationshipEligibilityFact::HasTrick {
                return target_facts.satisfies(fact, tick, ticks_per_day, target_types);
            }
            return self.relationship_satisfies(
                subject,
                subject_facts,
                target,
                target_facts,
                relationship,
                fact,
            );
        }
        if let Some(satisfies) = self.spatial_satisfies(target_facts, fact, day_fraction) {
            return satisfies;
        }
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let flag = |value: bool| if value { Q16_ONE } else { 0 };
        let actual = match fact.fact_input {
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InHabitat) => {
                flag(
                    subject_facts.habitat_entity().is_some()
                        && subject_facts.habitat_entity() == target_facts.habitat_entity(),
                )
            }
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::CanMate) => {
                flag(self.can_mate(subject, target, target_facts))
            }
            BehaviorEligibilityFactInput::Spatial(
                kind @ (BehaviorSpatialEligibilityFact::InSight
                | BehaviorSpatialEligibilityFact::NotInSight),
            ) => {
                let inside = subject_facts
                    .transform
                    .translation()
                    .xz()
                    .distance(target_facts.transform.translation().xz())
                    < expected as f32 / 65_536.0;
                return if kind == BehaviorSpatialEligibilityFact::InSight {
                    inside
                } else {
                    !inside
                };
            }
            _ => return target_facts.satisfies(fact, tick, ticks_per_day, target_types),
        };
        compare_q16(actual, expected, fact.comparison)
    }

    fn relationship_satisfies(
        &self,
        subject: Entity,
        subject_facts: &AnimalCandidateFactsItem,
        target: Entity,
        target_facts: &AnimalCandidateFactsItem,
        relationship: BehaviorRelationshipEligibilityFact,
        fact: &BehaviorCandidateEligibilityFact,
    ) -> bool {
        let holds = match relationship {
            BehaviorRelationshipEligibilityFact::SameSpecies => {
                subject_facts.species.species == target_facts.species.species
            }
            BehaviorRelationshipEligibilityFact::HasTrick => return false,
            BehaviorRelationshipEligibilityFact::Family
            | BehaviorRelationshipEligibilityFact::Relation => {
                let parents = self.lineage.parents(subject);
                let target_parents = self.lineage.parents(target);
                let mother = parents.mother == Some(target);
                let father = parents.father == Some(target);
                let child = target_parents.mother == Some(subject)
                    || target_parents.father == Some(subject);
                let sibling = subject != target
                    && (parents
                        .mother
                        .is_some_and(|mother| Some(mother) == target_parents.mother)
                        || parents
                            .father
                            .is_some_and(|father| Some(father) == target_parents.father));
                let mate = subject_facts.mate.is_some_and(|mate| mate.0 == target);
                let grandparent = |parent: Option<Entity>, maternal: bool| {
                    parent.is_some_and(|parent| {
                        let parents = self.lineage.parents(parent);
                        (if maternal {
                            parents.mother
                        } else {
                            parents.father
                        }) == Some(target)
                    })
                };
                let relations = [
                    ("mother", mother),
                    ("father", father),
                    ("child", child),
                    ("sibling", sibling),
                    ("grandmm", grandparent(parents.mother, true)),
                    ("grandmf", grandparent(parents.mother, false)),
                    ("grandfm", grandparent(parents.father, true)),
                    ("grandff", grandparent(parents.father, false)),
                ];
                if relationship == BehaviorRelationshipEligibilityFact::Relation {
                    if let BehaviorEligibilityFactValue::Symbol(expected) = fact.expected_value {
                        let found = (mate && expected == AssetId::from_key("mate"))
                            || relations.iter().any(|(name, related)| {
                                *related && expected == AssetId::from_key(name)
                            });
                        return compare_q16(i32::from(found) * Q16_ONE, Q16_ONE, fact.comparison);
                    }
                    mate || relations.iter().any(|&(_, related)| related)
                } else {
                    relations.iter().any(|&(_, related)| related)
                }
            }
        };
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        compare_q16(i32::from(holds) * Q16_ONE, expected, fact.comparison)
    }
}
