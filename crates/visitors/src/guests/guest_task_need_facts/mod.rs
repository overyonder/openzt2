//! Borrowed guest needs and the shared BFAI trigger/cessation contract.

use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorConditionEligibilityFact,
        BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
        BehaviorEligibilityFactValue, BehaviorSpatialEligibilityFact,
        BehaviorWelfareEligibilityFact,
    },
    world_definitions::guest_simulation_definitions::GuestNeedKind,
    AssetId,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::behavior_token_types::EducationDonationBonus;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;

use super::guest_simulation_types::*;

#[derive(QueryData)]
pub(super) struct GuestTaskNeedFacts {
    hunger: &'static GuestHunger,
    thirst: &'static GuestThirst,
    dessert: &'static GuestDessert,
    gift: &'static GuestGift,
    rest: &'static GuestEnergy,
    bathroom: &'static GuestRestroom,
    social: &'static GuestSocial,
    amusement: &'static GuestAmusement,
    viewing: Option<&'static GuestViewingNeed>,
    happiness: Option<&'static GuestSatisfaction>,
    donation_bonus: Has<EducationDonationBonus>,
}

impl GuestTaskNeedFactsItem<'_, '_> {
    /// `subject_types` holds the archetype's authored type flags, such as the
    /// `b_NoBuyHeadGift` a costumed guest carries.
    pub(super) fn satisfies(
        &self,
        fact: &BehaviorCandidateEligibilityFact,
        triggers: &GuestNeedTriggers,
        subject_types: &[AssetId],
    ) -> bool {
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let actual = match fact.fact_input {
            // This qualifier is unconditional.
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface,
            ) => 1 << 16,
            BehaviorEligibilityFactInput::Welfare(need) => {
                let name = match need {
                    BehaviorWelfareEligibilityFact::Hunger => "hunger",
                    BehaviorWelfareEligibilityFact::Thirst => "thirst",
                    BehaviorWelfareEligibilityFact::Dessert => "dessert",
                    BehaviorWelfareEligibilityFact::Rest => "rest",
                    BehaviorWelfareEligibilityFact::Bathroom => "bathroom",
                    BehaviorWelfareEligibilityFact::Social => "social",
                    BehaviorWelfareEligibilityFact::Amusement => "amusement",
                    BehaviorWelfareEligibilityFact::Happiness => "happiness",
                    _ => return false,
                };
                let Some((value, _)) = self.source_need(AssetId::from_key(name), triggers) else {
                    return false;
                };
                value
            }
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::DonationBonus,
            ) => i32::from(self.donation_bonus) * Q16_ONE,
            BehaviorEligibilityFactInput::Condition(condition) => {
                let flag = match condition {
                    BehaviorConditionEligibilityFact::NoBuyBackGift => "b_nobuybackgift",
                    BehaviorConditionEligibilityFact::NoBuyHeadGift => "b_nobuyheadgift",
                    BehaviorConditionEligibilityFact::NoBuyLeftHandGift => "b_nobuylhandgift",
                    BehaviorConditionEligibilityFact::NoBuyRightHandGift => "b_nobuyrhandgift",
                    BehaviorConditionEligibilityFact::NoBuyShirtGift => "b_nobuyshirtgift",
                    BehaviorConditionEligibilityFact::NoBuyWaistGift => "b_nobuywaistgift",
                    _ => return false,
                };
                if subject_types.contains(&AssetId::from_key(flag)) {
                    1 << 16
                } else {
                    0
                }
            }
            _ => return false,
        };
        match fact.comparison {
            BehaviorEligibilityFactComparison::Equal => actual == expected,
            BehaviorEligibilityFactComparison::NotEqual => actual != expected,
            BehaviorEligibilityFactComparison::Less => actual < expected,
            BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
            BehaviorEligibilityFactComparison::Greater => actual > expected,
            BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
        }
    }

    fn wellness_q16(&self, kind: GuestNeedKind) -> i32 {
        let (value, residual) = match kind {
            GuestNeedKind::Hunger => (self.hunger.value, self.hunger.residual_q16),
            GuestNeedKind::Thirst => (self.thirst.value, self.thirst.residual_q16),
            GuestNeedKind::Dessert => (self.dessert.value, self.dessert.residual_q16),
            GuestNeedKind::Gift => (self.gift.value, self.gift.residual_q16),
            GuestNeedKind::Energy => (self.rest.value, self.rest.residual_q16),
            GuestNeedKind::Restroom => (self.bathroom.value, self.bathroom.residual_q16),
            GuestNeedKind::Social => (self.social.value, self.social.residual_q16),
            GuestNeedKind::Amusement => (self.amusement.value, self.amusement.residual_q16),
        };
        (i32::from(value) * Q16_ONE + i32::from(residual)).clamp(0, MAX_NEED_Q16)
    }

    pub(super) fn source_need(
        &self,
        attribute: AssetId,
        triggers: &GuestNeedTriggers,
    ) -> Option<(i32, bool)> {
        if attribute == AssetId::from_key("happiness") {
            return self
                .happiness
                .filter(|_| triggers.happiness_authored)
                .map(|need| {
                    (
                        need.source_q16(),
                        triggers.state.0 & (1 << HAPPINESS_TRIGGER_BIT) != 0,
                    )
                });
        }
        if attribute == AssetId::from_key("viewanimals") {
            return self.viewing.map(|need| {
                (
                    need.value_q16,
                    triggers.state.0 & (1 << VIEWING_TRIGGER_BIT) != 0,
                )
            });
        }
        [
            ("hunger", GuestNeedKind::Hunger),
            ("thirst", GuestNeedKind::Thirst),
            ("dessert", GuestNeedKind::Dessert),
            ("gift", GuestNeedKind::Gift),
            ("rest", GuestNeedKind::Energy),
            ("bathroom", GuestNeedKind::Restroom),
            ("social", GuestNeedKind::Social),
            ("amusement", GuestNeedKind::Amusement),
        ]
        .into_iter()
        .find_map(|(name, kind)| {
            (attribute == AssetId::from_key(name)).then(|| {
                (
                    (MAX_NEED_Q16 - self.wellness_q16(kind)) / 10,
                    triggers.state.0 & (1 << need_index(kind)) != 0,
                )
            })
        })
    }
}

const VIEWING_TRIGGER_BIT: usize = 8;
const HAPPINESS_TRIGGER_BIT: usize = 9;

const fn need_index(kind: GuestNeedKind) -> usize {
    kind as usize
}

#[allow(clippy::type_complexity)]
pub(super) fn observe_guest_need_triggers(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut asset_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    changed_guests: Query<
        Entity,
        (
            With<Guest>,
            Or<(
                Added<Guest>,
                Changed<GuestArchetype>,
                Changed<GuestHunger>,
                Changed<GuestThirst>,
                Changed<GuestDessert>,
                Changed<GuestGift>,
                Changed<GuestEnergy>,
                Changed<GuestRestroom>,
                Changed<GuestSocial>,
                Changed<GuestAmusement>,
                Changed<GuestViewingNeed>,
                Changed<GuestSatisfaction>,
            )>,
        ),
    >,
    (mut removed_viewing, mut removed_happiness): (
        RemovedComponents<GuestViewingNeed>,
        RemovedComponents<GuestSatisfaction>,
    ),
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            GuestTaskNeedFacts,
            Option<&mut GuestNeedTriggers>,
        ),
        With<Guest>,
    >,
    mut commands: Commands,
) {
    let assets_changed = asset_events.read().fold(false, |changed, event| {
        changed
            | matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
    });
    let refresh_all = definitions.is_changed() || assets_changed;
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let removed_viewing = removed_viewing.read();
    let removed_happiness = removed_happiness.read();
    let candidates: bevy::ecs::entity::EntityHashSet = if refresh_all {
        guests.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed_guests
            .iter()
            .chain(removed_viewing)
            .chain(removed_happiness)
            .collect()
    };
    for candidate in candidates {
        let Ok((entity, archetype, facts, triggers)) = guests.get_mut(candidate) else {
            continue;
        };
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let mut state = GuestNeedTriggers {
            state: triggers
                .as_deref()
                .map_or_else(Default::default, |triggers| triggers.state),
            happiness_authored: definition.happiness_need.is_some(),
        };
        for need in &definition.needs {
            state.state.observe(
                need_index(need.kind),
                facts.wellness_q16(need.kind),
                Some(1000_u16.saturating_sub(need.reconsider_threshold)),
                need.cessation_threshold
                    .map(|value| 1000_u16.saturating_sub(value)),
            );
        }
        // This need preserves signed authored thresholds; -100 must not be
        // clamped to zero by the unsigned wellness needs.
        if let Some(policy) = definition.viewing_need {
            let value = facts.viewing.map_or_else(
                || GuestViewingNeed::new(policy.initial_q16).value_q16,
                |need| need.value_q16,
            );
            if policy
                .trigger_q16
                .is_some_and(|threshold| value >= threshold)
            {
                state.state.0 |= 1 << VIEWING_TRIGGER_BIT;
            } else if policy
                .cessation_q16
                .is_some_and(|threshold| value <= threshold)
            {
                state.state.0 &= !(1 << VIEWING_TRIGGER_BIT);
            }
            if facts.viewing.is_none() {
                commands.entity(entity).insert(GuestViewingNeed::new(value));
            }
        } else {
            state.state.0 &= !(1 << VIEWING_TRIGGER_BIT);
            if facts.viewing.is_some() {
                commands.entity(entity).remove::<GuestViewingNeed>();
            }
        }
        if let (Some(policy), Some(happiness)) = (definition.happiness_need, facts.happiness) {
            let value = happiness.source_q16();
            if policy
                .trigger_q16
                .is_some_and(|threshold| value >= threshold)
            {
                state.state.0 |= 1 << HAPPINESS_TRIGGER_BIT;
            } else if policy
                .cessation_q16
                .is_some_and(|threshold| value <= threshold)
            {
                state.state.0 &= !(1 << HAPPINESS_TRIGGER_BIT);
            }
        } else {
            state.state.0 &= !(1 << HAPPINESS_TRIGGER_BIT);
        }
        if let Some(mut triggers) = triggers {
            triggers
                .reborrow()
                .map_unchanged(|triggers| &mut triggers.state)
                .set_if_neq(state.state);
            triggers
                .map_unchanged(|triggers| &mut triggers.happiness_authored)
                .set_if_neq(state.happiness_authored);
        } else {
            commands.entity(entity).insert(state);
        }
    }
}
