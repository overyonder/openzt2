use super::guest_definition_queries::guest_need_rows;
use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestRarity;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use super::guest_simulation_calculations::apply_q16;
use super::guest_simulation_types::AdjustGuestSatisfaction;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestAmusement;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::{GuestNeedAdjustmentDue, GuestNeedAdjustmentTimer};
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestRng;
use super::guest_simulation_types::GuestSatisfaction;
use super::guest_simulation_types::GuestSocial;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingNeed;
use super::guest_simulation_types::VisitTime;
use openzt2_zoo_state::shows::running_show_types::GuestShowNeed;

pub(crate) fn apply_guest_satisfaction_adjustments(
    mut adjustments: MessageReader<AdjustGuestSatisfaction>,
    mut guests: Query<&mut GuestSatisfaction, With<Guest>>,
) {
    for adjustment in adjustments.read() {
        let Ok(mut satisfaction) = guests.get_mut(adjustment.guest) else {
            continue;
        };
        // The message is wellness percentage points; retain its fractional
        // permille in the canonical component instead of truncating each event.
        satisfaction.add_wellness_q16(adjustment.delta_q16.saturating_mul(10));
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn tick_guest_need_cadences(
    time: Res<Time<Fixed>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut guests: Query<
        (Entity, &mut GuestNeedAdjustmentTimer, &mut GuestRng),
        (
            With<Guest>,
            With<GuestArchetype>,
            With<VisitTime>,
            With<GuestHunger>,
            With<GuestThirst>,
            With<GuestDessert>,
            With<GuestGift>,
            With<GuestEnergy>,
            With<GuestRestroom>,
            With<GuestSocial>,
            With<GuestAmusement>,
            With<GuestSatisfaction>,
        ),
    >,
    mut due: MessageWriter<GuestNeedAdjustmentDue>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions
        .need_adjustment()
        .filter(|policy| policy.enabled)
    else {
        return;
    };
    for (entity, mut timer, mut rng) in &mut guests {
        if openzt2_zoo_state::need_adjustment::tick_need_adjustment(
            &mut timer.0,
            time.timestep(),
            policy.delay_steps,
            &mut rng.0,
        ) {
            due.write(GuestNeedAdjustmentDue(entity));
        }
    }
}

pub(super) fn initialize_guest_show_need(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<
        (Entity, &GuestArchetype, &mut GuestRng),
        (With<Guest>, Without<GuestShowNeed>),
    >,
    mut commands: Commands,
) {
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    for (guest, archetype, mut random) in &mut guests {
        let Some(policy) = definitions.guest_show_need(archetype.0) else {
            continue;
        };
        let source = |q16: i32| q16 as f32 / 65_536.0;
        let Some(trigger) = policy.trigger_q16.map(source) else {
            continue;
        };
        let Some(cessation) = policy.cessation_q16.map(source) else {
            continue;
        };
        let [minimum, maximum] = policy.initial_q16.map(source);
        let range = (minimum < maximum).then(|| (maximum - minimum) as u32);
        commands.entity(guest).insert(GuestShowNeed::from_authored(
            minimum,
            range,
            trigger,
            cessation,
            || random.0.range_u32(32_768).unwrap_or_default() as u16,
        ));
    }
}

pub(super) fn adjust_due_guest_show_needs(
    mut due: MessageReader<GuestNeedAdjustmentDue>,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<(&GuestArchetype, &mut GuestShowNeed), With<Guest>>,
) {
    let Some(definitions) = active.get(&assets) else {
        due.clear();
        return;
    };
    for event in due.read() {
        let Ok((archetype, mut need)) = guests.get_mut(event.0) else {
            continue;
        };
        if let Some(policy) = definitions.guest_show_need(archetype.0) {
            need.adjust(policy.adjustment_q16 as f32 / 65_536.0);
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn decay_due_guest_needs(
    mut due: MessageReader<GuestNeedAdjustmentDue>,
    rates: Res<GuestArchetypeTables>,
    mut guests: Query<
        (
            &GuestArchetype,
            &mut GuestHunger,
            &mut GuestThirst,
            &mut GuestDessert,
            &mut GuestGift,
            &mut GuestEnergy,
            &mut GuestRestroom,
            &mut GuestSocial,
            &mut GuestAmusement,
            Option<&mut GuestViewingNeed>,
            &mut GuestSatisfaction,
        ),
        With<Guest>,
    >,
) {
    for adjustment in due.read() {
        let Ok((
            archetype,
            mut hunger,
            mut thirst,
            mut dessert,
            mut gift,
            mut energy,
            mut restroom,
            mut social,
            mut amusement,
            viewing_need,
            mut satisfaction,
        )) = guests.get_mut(adjustment.0)
        else {
            continue;
        };
        let Some(needs) = rates.rates.get(&archetype.0) else {
            continue;
        };
        if let Some(adjustment) = needs.happiness {
            satisfaction.add_wellness_q16(adjustment);
        }
        if let (Some(mut need), Some(adjustment)) = (viewing_need, needs.viewing) {
            need.add_source_delta(adjustment);
        }
        let ordered = needs.ordered;
        let hunger = &mut *hunger;
        apply_q16(&mut hunger.value, &mut hunger.residual_q16, ordered[0]);
        let thirst = &mut *thirst;
        apply_q16(&mut thirst.value, &mut thirst.residual_q16, ordered[1]);
        let dessert = &mut *dessert;
        apply_q16(&mut dessert.value, &mut dessert.residual_q16, ordered[2]);
        let gift = &mut *gift;
        apply_q16(&mut gift.value, &mut gift.residual_q16, ordered[3]);
        let energy = &mut *energy;
        apply_q16(&mut energy.value, &mut energy.residual_q16, ordered[4]);
        let restroom = &mut *restroom;
        apply_q16(&mut restroom.value, &mut restroom.residual_q16, ordered[5]);
        let social = &mut *social;
        apply_q16(&mut social.value, &mut social.residual_q16, ordered[6]);
        let amusement = &mut *amusement;
        apply_q16(
            &mut amusement.value,
            &mut amusement.residual_q16,
            ordered[7],
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct NeedRates {
    ordered: [i32; 8],
    viewing: Option<i32>,
    happiness: Option<i32>,
}

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct GuestArchetypeTables {
    rates: HashMap<AssetId, NeedRates>,
    rarity: [[Vec<AssetId>; 3]; 4],
}

const GUEST_DEMOGRAPHICS: [&str; 4] = [
    "guest_adult_m",
    "guest_adult_f",
    "guest_young_m",
    "guest_young_f",
];

impl GuestArchetypeTables {
    pub(super) fn choose_archetype(
        &self,
        test_type: bool,
        [rare_threshold, uncommon_threshold]: [u16; 2],
        mut draw: impl FnMut() -> u16,
    ) -> Option<AssetId> {
        if test_type {
            return Some(AssetId::from_key(GUEST_DEMOGRAPHICS[0]));
        }
        let demographic = &self.rarity[usize::from(draw() % 4)];
        let roll = draw() % 100;
        let rare = &demographic[GuestRarity::Rare as usize];
        let uncommon = &demographic[GuestRarity::Uncommon as usize];
        let common = &demographic[GuestRarity::Common as usize];
        let cohort = if roll < rare_threshold && !rare.is_empty() {
            rare
        } else if roll < uncommon_threshold && !uncommon.is_empty() {
            uncommon
        } else {
            common
        };
        (!cohort.is_empty()).then(|| cohort[usize::from(draw()) % cohort.len()])
    }
}

#[cfg(test)]
mod arrival_cohort_tests {
    use super::*;

    #[test]
    fn test_type_draws_nothing_and_rarity_boundaries_are_strict() {
        let mut tables = GuestArchetypeTables::default();
        let uncommon = AssetId::from_key("uncommon");
        let common = AssetId::from_key("common");
        tables.rarity[0][GuestRarity::Uncommon as usize].push(uncommon);
        tables.rarity[0][GuestRarity::Common as usize].push(common);
        assert_eq!(
            tables.choose_archetype(true, [10, 30], || panic!("test type drew")),
            Some(AssetId::from_key("guest_adult_m"))
        );
        for (roll, expected) in [(10, uncommon), (30, common)] {
            let mut draws = [0, roll, 0].into_iter();
            assert_eq!(
                tables.choose_archetype(false, [10, 30], || draws.next().unwrap()),
                Some(expected)
            );
            assert_eq!(draws.next(), None);
        }
    }
}

pub(super) fn compile_guest_archetype_tables(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut rates: ResMut<GuestArchetypeTables>,
) {
    if !assets.is_changed() && !definitions.is_changed() {
        return;
    }
    let mut resolved = GuestArchetypeTables::default();
    if let Some(definitions) = definitions.get(&assets) {
        for definition in definitions.guest_definitions() {
            if let Some(demographic) = (definition.id != AssetId::from_key(GUEST_DEMOGRAPHICS[0]))
                .then(|| {
                    GUEST_DEMOGRAPHICS.iter().position(|family| {
                        definition
                            .behavior_subject_type_identifiers
                            .contains(&AssetId::from_key(family))
                    })
                })
                .flatten()
            {
                resolved.rarity[demographic][definition.rarity as usize].push(definition.id);
            }
            let Some(needs) = guest_need_rows(definition) else {
                continue;
            };
            resolved.rates.insert(
                definition.id,
                NeedRates {
                    ordered: needs.map(|need| need.adjustment_q16),
                    viewing: definition.viewing_need.map(|need| need.adjustment_q16),
                    happiness: definition
                        .happiness_need
                        .map(|need| openzt2_zoo_state::need_adjustment::wellness_delta_from_authored_deprivation_q16(need.adjustment_q16)),
                },
            );
        }
    }
    rates.set_if_neq(resolved);
}
