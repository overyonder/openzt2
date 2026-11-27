use bevy::prelude::*;
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_game_data::{
    world_definitions::guest_simulation_definitions::GuestEducationDonationPolicy, AssetId,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{
        BehaviorSelfToken, BehaviorTokenEconomyTerms, EducationDonationBonus,
    },
    donations::donation_guest_decision_types::{
        GuestDonationMeters, SpeciesDonationCandidateLifetimes,
    },
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::GuestSatisfaction,
};

use super::donation_opportunity_observation::SPENT_SPECIES_SCORE;

pub(crate) fn offer_view_donations(
    guest: Entity,
    policy: GuestEducationDonationPolicy,
    meters: &mut GuestDonationMeters,
    lifetimes: &mut SpeciesDonationCandidateLifetimes,
    species: Option<SpeciesView<'_>>,
    wallet: &Wallet,
    happiness: &GuestSatisfaction,
    bonus: bool,
    tick: u64,
    timestep: std::time::Duration,
    commands: &mut Commands,
) {
    let education = AssetId::from_key("donate_education");
    let need = happiness.source_q16() as f32 / 65_536.0;
    let cash = wallet.0.dollars();
    let timeout = policy.timeout.unwrap_or(60.0);
    let timeout = if timeout == -1.0 {
        1_000_000_000.0
    } else {
        timeout
    };
    let expiry = tick.saturating_add((f64::from(timeout) / timestep.as_secs_f64()).ceil() as u64);
    let issue_token = |name, tracker, amount, commands: &mut Commands| {
        commands.spawn((
            ChildOf(guest),
            BehaviorSelfToken {
                name,
                available_at: tick,
                expires_at: expiry,
                payload: None,
                retain_on_failure: false,
            },
            BehaviorTokenEconomyTerms {
                cost_override: amount,
                tracker,
            },
        ));
    };

    if let Some(species) = species {
        let mut scores: Vec<_> = meters
            .0
            .iter_mut()
            .filter(|(key, score)| **key != education && **score != SPENT_SPECIES_SCORE)
            .collect();
        scores.sort_unstable_by_key(|(species, _)| **species);
        for (&id, score) in scores {
            let Some(animal) = species.find(id) else {
                continue;
            };
            let candidate = lifetimes.0.get(&id);
            let Some(terms) = candidate
                .and_then(|candidate| species.donation_terms(candidate.binder))
                .or_else(|| species.donation_terms(id))
            else {
                continue;
            };
            let Some(token) = terms.token else { continue };
            let cap = terms.base_amount_cents as f32 / 100.0 * terms.maximum_species_factor;
            let amount = donation_amount(*score, policy, need, false);
            if let Some(amount) = species_offer(
                amount,
                cap,
                candidate.map(|candidate| candidate.remaining),
                cash,
            ) {
                issue_token(
                    token,
                    AssetId::from_key(&format!("Donate_{}", animal.authored_species_name)),
                    amount,
                    commands,
                );
                *score = SPENT_SPECIES_SCORE;
            }
        }
    }
    if let Some(score) = meters.0.get_mut(&education) {
        let amount = donation_amount(*score, policy, need, true);
        if let Some(amount) = education_attempt(score, amount, bonus, policy.bonus_factor) {
            if bonus {
                commands
                    .entity(guest)
                    .try_remove::<EducationDonationBonus>();
            }
            if cash >= amount {
                issue_token(AssetId::from_key("t_donateed"), education, amount, commands);
            }
        }
    }
    lifetimes.0.retain(|_, candidate| candidate.remaining > 0.0);
}

fn donation_amount(
    score: f32,
    policy: GuestEducationDonationPolicy,
    happiness: f32,
    education: bool,
) -> f32 {
    if !policy.divisor.is_finite() || policy.divisor <= 0.0 {
        return 0.0;
    }
    let mut amount = score / policy.divisor;
    if education {
        if amount < policy.minimum {
            return 0.0;
        }
        if let Some(maximum) = policy.maximum {
            amount = amount.min(maximum);
        }
    }
    if amount > 0.0
        && policy
            .happiness_threshold
            .is_some_and(|threshold| happiness > threshold)
    {
        amount *= (100.0 - happiness) / 100.0;
    }
    amount
}

fn species_offer(amount: f32, cap: f32, candidate_lifetime: Option<f32>, cash: f32) -> Option<f32> {
    if !(amount > 0.0 && cap > 0.0) {
        return None;
    }
    let amount = amount.min(cap);
    (amount > 0.0
        && cash >= amount
        && (amount < cap || candidate_lifetime.is_some_and(|lifetime| lifetime <= 0.0)))
    .then_some(amount)
}

fn education_attempt(score: &mut f32, amount: f32, bonus: bool, multiplier: f32) -> Option<f32> {
    if amount > 0.0 {
        *score = 0.0;
        Some(if bonus { amount * multiplier } else { amount })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADULT: GuestEducationDonationPolicy = GuestEducationDonationPolicy {
        divisor: 5.0,
        minimum: 5.0,
        maximum: Some(1000.0),
        happiness_threshold: Some(70.0),
        bonus_factor: 3.0,
        timeout: None,
    };

    #[test]
    fn species_scores_use_happiness_above_the_threshold_and_require_cash() {
        let at_threshold = donation_amount(100.0, ADULT, 70.0, false);
        let unhappy = donation_amount(100.0, ADULT, 80.0, false);
        assert_eq!(species_offer(at_threshold, 500.0, None, 20.0), Some(20.0));
        assert_eq!(species_offer(unhappy, 500.0, None, 100.0), Some(4.0));
        assert_eq!(species_offer(unhappy, 500.0, None, 3.0), None);
    }

    #[test]
    fn education_bonus_and_score_are_consumed_on_a_positive_attempt_before_wallet_check() {
        let mut score = 25.0;
        let amount = donation_amount(score, ADULT, 70.0, true);
        assert_eq!(education_attempt(&mut score, amount, true, 3.0), Some(15.0));
        assert_eq!(score, 0.0);
        let mut score = 24.0;
        let amount = donation_amount(score, ADULT, 70.0, true);
        assert_eq!(education_attempt(&mut score, amount, false, 3.0), None);
        assert_eq!(score, 24.0);
    }

    #[test]
    fn reaching_the_species_cap_requires_an_expired_candidate_row() {
        assert_eq!(species_offer(30.0, 20.0, Some(0.0), 100.0), Some(20.0));
        assert_eq!(species_offer(30.0, 20.0, Some(0.01), 100.0), None);
        assert_eq!(species_offer(30.0, 20.0, None, 100.0), None);
    }
}
