use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::BehaviorSelfToken,
    donations::donation_opportunity_types::DonationOpportunity,
    economy::{authored_transaction_amount::authored_currency_to_money, money_types::Money},
    transport_tours::transport_circuit_types::TransportCircuit,
};

const GUEST_SOURCE: &str = "entities/units/guests/ai/guest.xml";

/// Transaction arguments attached to the guest's timed donation token.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TourDonationOffer {
    pub cost_override: Money,
    pub transaction_tracker: AssetId,
    pub circuit: Entity,
}

#[derive(Asset, TypePath)]
pub(super) struct TourDonationPolicy {
    timeout_seconds: f64,
    factor: f32,
    cap: f32,
    feedback: Vec<TourFeedbackBand>,
}

struct TourFeedbackBand {
    score: [f32; 2],
    happiness_q16: i32,
    departure_q16: i32,
}

#[derive(Default, TypePath)]
pub(super) struct TourDonationPolicyLoader;

impl AssetLoader for TourDonationPolicyLoader {
    type Asset = TourDonationPolicy;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<TourDonationPolicy> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let source = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let policy = source
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("shared"))
            .and_then(|shared| {
                shared
                    .element_children()
                    .find(|node| node.name.eq_ignore_ascii_case("ZTAISharedTourTaker"))
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing shared tour policy")
            })?;
        let read = |name| {
            policy
                .attribute_named_any(&[name])
                .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid tour policy field {name}"),
                    )
                })
        };
        let mut feedback = Vec::new();
        for row in policy
            .element_children()
            .filter(|node| node.name.eq_ignore_ascii_case("ZTAITourTakerData"))
            .flat_map(|node| node.element_children())
            .filter(|node| node.name.eq_ignore_ascii_case("range"))
        {
            let number = |name| {
                row.attribute_named_any(&[name])
                    .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            };
            let effect = row
                .element_children()
                .find(|node| node.name.eq_ignore_ascii_case("BFAISubjectData"));
            let q16 = |name| {
                effect
                    .and_then(|effect| effect.attribute_named_any(&[name]))
                    .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                    .filter(|value| value.is_finite())
                    .map_or(0, |value| (value * 65_536.0).round() as i32)
            };
            feedback.push(TourFeedbackBand {
                score: [
                    number("Min").unwrap_or(f32::NEG_INFINITY),
                    number("Max").unwrap_or(f32::INFINITY),
                ],
                happiness_q16: q16("happiness"),
                departure_q16: q16("f_DeparturePoints"),
            });
        }
        Ok(TourDonationPolicy {
            timeout_seconds: f64::from(read("tourTokenTimeOut")?),
            factor: read("donationScaleFactor")?,
            cap: read("donationCap")?,
            feedback,
        })
    }
}

#[derive(Resource)]
pub(super) struct TourDonationPolicySource(Handle<TourDonationPolicy>);

pub(super) fn load_tour_donation_policy(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(TourDonationPolicySource(assets.load(GUEST_SOURCE)));
}

pub(super) fn tour_donation_opportunity() -> DonationOpportunity {
    DonationOpportunity {
        category: AssetId::from_key("tour-donation"),
        amount: Money::ZERO,
        interaction_ticks: 0,
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct TourDonation<'w, 's> {
    clocks: Query<'w, 's, &'static ZooClock>,
    time: Res<'w, Time<Fixed>>,
    source: Res<'w, TourDonationPolicySource>,
    policies: Res<'w, Assets<TourDonationPolicy>>,
    subjects:
        Query<'w, 's, &'static WorldMember, (With<TransportCircuit>, With<DonationOpportunity>)>,
}

impl TourDonation<'_, '_> {
    pub(super) fn ride_feedback(&self, score: f32) -> Option<(i32, i32)> {
        let band = self
            .policies
            .get(&self.source.0)?
            .feedback
            .iter()
            .find(|band| band.score[0] <= score && score < band.score[1])?;
        Some((band.happiness_q16, band.departure_q16))
    }

    pub(super) fn offer(
        &self,
        commands: &mut Commands,
        (guest, world, budget): (Entity, WorldMember, Money),
        circuit: Entity,
        contributions: [f32; 2],
    ) {
        let Some(policy) = self.policies.get(&self.source.0) else {
            return;
        };
        let score = contributions[0] + contributions[1];
        if !score.is_finite()
            || !self
                .subjects
                .get(circuit)
                .is_ok_and(|member| member.root == world.root)
        {
            return;
        }
        let dollars = (score * policy.factor).min(policy.cap);
        let Some(amount) = authored_currency_to_money(f64::from(dollars))
            .filter(|amount| dollars >= 1.0 && *amount <= budget)
        else {
            return;
        };
        let Ok(clock) = self.clocks.get(world.root) else {
            return;
        };
        let tick = clock.tick;
        let lifetime = (policy.timeout_seconds / self.time.timestep().as_secs_f64()).ceil() as u64;
        commands.spawn((
            world,
            ChildOf(guest),
            TourDonationOffer {
                cost_override: amount,
                transaction_tracker: AssetId::from_key("donate_tour"),
                circuit,
            },
            BehaviorSelfToken {
                name: AssetId::from_key("t_donationtour"),
                available_at: tick,
                expires_at: tick.saturating_add(lifetime),
                payload: None,
                retain_on_failure: false,
            },
        ));
    }
}
