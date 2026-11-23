use std::io;

use super::transport_fare_types::PendingTransportFare;
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
    donations::{
        donation_opportunity_types::DonationOpportunity,
        donation_payment_types::{
            DonationPaymentRequested, DonationPaymentState, PendingDonationPayment,
        },
    },
    economy::{
        account_transaction_types::{Account, TransactionKind, TransactionRequest},
        authored_transaction_amount::authored_currency_to_money,
        money_types::Money,
    },
    transport_tours::transport_circuit_types::TransportCircuit,
};

const GUEST_SOURCE: &str = "entities/units/guests/ai/guest.xml";

#[derive(Asset, TypePath)]
pub(super) struct TourDonationPolicy {
    timeout_seconds: f64,
    factor: f32,
    cap: f32,
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
        Ok(TourDonationPolicy {
            timeout_seconds: f64::from(read("tourTokenTimeOut")?),
            factor: read("donationScaleFactor")?,
            cap: read("donationCap")?,
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
    pub(super) fn request_at_station(
        &self,
        commands: &mut Commands,
        transactions: &mut MessageWriter<TransactionRequest>,
        (guest, world, budget): (Entity, WorldMember, Money),
        fare: PendingTransportFare,
    ) {
        let PendingTransportFare {
            station,
            circuit,
            score,
            ..
        } = fare;
        let Some(policy) = self.policies.get(&self.source.0) else {
            return;
        };
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
        let operation = commands
            .spawn((
                world,
                PendingDonationPayment {
                    guest,
                    acceptor: station,
                    subject: circuit,
                    amount,
                },
                DonationPaymentState::Pending,
            ))
            .id();
        commands
            .entity(guest)
            .insert(DonationPaymentRequested { submitted: true });
        transactions.write(TransactionRequest {
            operation,
            debit: Account::Entity(guest),
            credit: Account::Zoo,
            amount,
            kind: TransactionKind::Donation,
            subject: Some(station),
        });
        let lifetime = (policy.timeout_seconds / self.time.timestep().as_secs_f64()).ceil() as u64;
        commands.spawn((
            ChildOf(guest),
            BehaviorSelfToken {
                name: AssetId::from_key("t_donationtour"),
                available_at: tick,
                expires_at: tick.saturating_add(lifetime),
                payload: Some(station),
                retain_on_failure: false,
            },
        ));
    }
}
