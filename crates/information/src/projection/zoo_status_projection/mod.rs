use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::progression::profile_challenge_types::TotalEndangeredAnimalBirthCount;
use openzt2_zoo_state::progression::rating_types::ZooRating;

pub(crate) fn project_live_zoo_status_to_authored_information_bindings(
    cash: Option<Single<Ref<ZooCash>>>,
    worlds: Query<
        (Option<Ref<Fame>>, Option<Ref<ZooRating>>),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    endangered_births: Option<Single<Ref<TotalEndangeredAnimalBirthCount>>>,
    mut values: Query<(Ref<UiValueBinding>, &mut UiValue, &mut Visibility)>,
    history: Option<
        Single<Ref<openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory>>,
    >,
    mut guest_texts: Query<(
        Ref<openzt2_ui::authored_ui_text_content_binding::UiTextBinding>,
        &mut Text,
    )>,
) {
    let cash = cash.map(Single::into_inner);
    let endangered_births = endangered_births.map(Single::into_inner);
    let history = history.map(Single::into_inner);
    let (fame, rating) = worlds.single().unwrap_or_default();
    let guest_count = history
        .as_ref()
        .and_then(|history| history.current_month_finance())
        .map_or(0, |month| month.active_users);
    let guest_count_changed = history.as_ref().is_some_and(DetectChanges::is_changed);
    for (binding, text) in &mut guest_texts {
        if matches!(binding.0, UiTextPropertyBindingSource::ZooGuestCount { .. })
            && (guest_count_changed || binding.is_added())
        {
            super::text_replacement_operations::replace_projected_ui_text_if_changed(
                text,
                &guest_count.to_string(),
            );
        }
    }
    let facts_changed = guest_count_changed
        || cash.as_ref().is_some_and(DetectChanges::is_changed)
        || fame.as_ref().is_some_and(DetectChanges::is_changed)
        || rating.as_ref().is_some_and(DetectChanges::is_changed)
        || endangered_births
            .as_ref()
            .is_some_and(DetectChanges::is_changed);
    for (binding, mut value, mut visibility) in &mut values {
        if !facts_changed && !binding.is_changed() {
            continue;
        }
        let next = match &binding.0 {
            UiIntegerPropertyBindingSource::ZooCashCents => cash
                .as_ref()
                .map(|cash| super::text_replacement_operations::currency_cents(cash.0)),
            UiIntegerPropertyBindingSource::ZooFameHalfStars => {
                fame.as_ref().map(|fame| i64::from(fame.half_stars))
            }
            UiIntegerPropertyBindingSource::ZooRatingPermille => {
                let Some(rating) = &rating else { continue };
                visibility.set_if_neq(if rating.overall_available {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if !rating.overall_available {
                    continue;
                }
                Some(i64::from(rating.overall_permille))
            }
            UiIntegerPropertyBindingSource::ZooGuestCount => Some(i64::from(guest_count)),
            UiIntegerPropertyBindingSource::EndangeredBirthCount => {
                endangered_births.as_ref().map(|births| i64::from(births.0))
            }
            _ => continue,
        };
        if let Some(next) = next {
            value.set_if_neq(UiValue(next));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
    use openzt2_zoo_state::economy::money_types::Money;
    use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;

    #[test]
    fn zoo_guest_text_tracks_admitted_guests_and_departures() {
        let mut app = App::new();
        let mut history = MonthlyFinanceHistory::default();
        history.enter_calendar_month(0, Money::ZERO);
        let root = app.world_mut().spawn(history).id();
        app.add_systems(
            Update,
            project_live_zoo_status_to_authored_information_bindings,
        );
        let field = app
            .world_mut()
            .spawn((
                Text::new(""),
                UiTextBinding(UiTextPropertyBindingSource::ZooGuestCount {
                    format: Default::default(),
                }),
            ))
            .id();
        app.update();
        assert_eq!(app.world().get::<Text>(field).unwrap().0, "0");
        app.world_mut()
            .get_mut::<MonthlyFinanceHistory>(root)
            .unwrap()
            .record_guest_arrival();
        app.update();
        assert_eq!(app.world().get::<Text>(field).unwrap().0, "1");
        app.world_mut()
            .get_mut::<MonthlyFinanceHistory>(root)
            .unwrap()
            .record_guest_departure();
        app.update();
        assert_eq!(app.world().get::<Text>(field).unwrap().0, "0");
    }
}
