use std::fmt::Write;

use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId,
};

use openzt2_game_assets::localization::loaded_localization_queries::CurrencyLocalization;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::money_types::Money;

use super::donation_payment_types::{DonationCategoryTotals, DonationTotal, ZooDonationSummary};

/// The contribution already included in a root summary.
#[derive(Component)]
pub(super) struct BookedDonations {
    root: Entity,
    totals: DonationCategoryTotals,
}

pub(super) fn add_booked_donations(
    event: On<Add<BookedDonations>>,
    booked: Query<&BookedDonations, Allow<Disabled>>,
    mut summaries: Query<&mut ZooDonationSummary, Allow<Disabled>>,
) {
    if let Ok(booked) = booked.get(event.entity) {
        if let Ok(mut summary) = summaries.get_mut(booked.root) {
            for (category, total) in booked.totals.iter() {
                summary.add(category, total);
            }
        }
    }
}

pub(super) fn remove_booked_donations(
    event: On<Remove<BookedDonations>>,
    booked: Query<&BookedDonations, Allow<Disabled>>,
    mut summaries: Query<&mut ZooDonationSummary, Allow<Disabled>>,
) {
    if let Ok(booked) = booked.get(event.entity) {
        if let Ok(mut summary) = summaries.get_mut(booked.root) {
            for (category, total) in booked.totals.iter() {
                summary.subtract(category, total);
            }
        }
    }
}

pub(super) fn withdraw_disabled_donations(
    event: On<Add<Disabled>>,
    booked: Query<(), (With<BookedDonations>, Allow<Disabled>)>,
    mut commands: Commands,
) {
    if booked.contains(event.entity) {
        commands.entity(event.entity).remove::<BookedDonations>();
    }
}

/// Posts ordinary successful payments directly to the current acceptor's zoo.
#[derive(SystemParam)]
pub(super) struct DonationWorldDeltas<'w, 's> {
    members: Query<'w, 's, &'static WorldMember>,
    booked: Query<'w, 's, &'static mut BookedDonations>,
    summaries: Query<'w, 's, &'static mut ZooDonationSummary, Allow<Disabled>>,
}

impl DonationWorldDeltas<'_, '_> {
    pub(super) fn record(&mut self, acceptor: Entity, category: AssetId, amount: Money) {
        let Ok(member) = self.members.get(acceptor) else {
            return;
        };
        let Ok(mut booked) = self.booked.get_mut(acceptor) else {
            return;
        };
        if booked.root != member.root {
            return;
        }
        let Ok(mut summary) = self.summaries.get_mut(member.root) else {
            return;
        };
        if booked
            .totals
            .accumulate_completed_donation(category, amount)
            .is_some()
        {
            summary.add(category, DonationTotal { amount, count: 1 });
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn reconcile_donation_contributors(
    mut commands: Commands,
    changed: Query<
        Entity,
        (
            With<DonationCategoryTotals>,
            Or<(Changed<DonationCategoryTotals>, Changed<WorldMember>)>,
        ),
    >,
    mut contributions: Query<
        (
            Option<&WorldMember>,
            Option<&DonationCategoryTotals>,
            Option<&mut BookedDonations>,
        ),
        Or<(With<DonationCategoryTotals>, With<BookedDonations>)>,
    >,
    mut summaries: Query<&mut ZooDonationSummary, Allow<Disabled>>,
    mut removed_totals: RemovedComponents<DonationCategoryTotals>,
    mut left_world: RemovedComponents<WorldMember>,
    mut reenabled: RemovedComponents<Disabled>,
) {
    let affected: EntityHashSet = changed
        .iter()
        .chain(removed_totals.read())
        .chain(left_world.read())
        .chain(reenabled.read())
        .collect();
    for entity in affected {
        let Ok((member, totals, booked)) = contributions.get_mut(entity) else {
            continue;
        };
        let Some((member, totals)) = member
            .zip(totals)
            .filter(|(member, _)| summaries.contains(member.root))
        else {
            if booked.is_some() {
                commands.entity(entity).remove::<BookedDonations>();
            }
            continue;
        };
        if let Some(mut booked) = booked {
            if booked.root == member.root && &booked.totals == totals {
                continue;
            }
            if let Ok(mut previous) = summaries.get_mut(booked.root) {
                for (category, total) in booked.totals.iter() {
                    previous.subtract(category, total);
                }
            }
            if let Ok(mut next) = summaries.get_mut(member.root) {
                for (category, total) in totals.iter() {
                    next.add(category, total);
                }
            }
            booked.root = member.root;
            booked.totals.clone_from(totals);
        } else {
            commands.entity(entity).insert(BookedDonations {
                root: member.root,
                totals: totals.clone(),
            });
        }
    }
}

pub(super) fn project_zoo_donation_summary(
    currency: CurrencyLocalization,
    summaries: Query<Ref<ZooDonationSummary>, With<WorldRoot>>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Ok(summary) = summaries.single() else {
        return;
    };
    let refresh_all = summary.is_changed() || currency.is_changed();
    let localization = currency.loaded_text();

    for (binding, text) in &mut fields {
        if !refresh_all && !binding.is_changed() && !text.is_added() {
            continue;
        }
        let category = match &binding.0 {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationCount { category, .. }
            | UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount { category, .. } => {
                *category
            }
            _ => continue,
        };
        let Some(total) = summary.total(category) else {
            continue;
        };
        let mut rendered = String::new();
        match &binding.0 {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationCount { format, .. } => {
                if localization.is_none_or(|localization| {
                    localization
                        .write_localized_text_with_format_arguments(
                            *format,
                            &[LocalizationFormatArgument::Integer(i64::from(total.count))],
                            &mut rendered,
                        )
                        .is_err()
                }) {
                    let _ = write!(rendered, "{}", total.count);
                }
            }
            UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount {
                positive_format,
                ..
            } => {
                let _ = currency.write_amount(
                    (f64::from(total.amount.dollars()) * 100.0).round() as i64,
                    false,
                    &mut rendered,
                );
                if let Some(localization) = localization {
                    let unformatted = std::mem::take(&mut rendered);
                    if localization
                        .write_localized_text_with_format_arguments(
                            *positive_format,
                            &[LocalizationFormatArgument::Text(&unformatted)],
                            &mut rendered,
                        )
                        .is_err()
                    {
                        rendered = unformatted;
                    }
                }
            }
            _ => unreachable!(),
        }
        text.map_unchanged(|text| &mut text.0).set_if_neq(rendered);
    }
}
