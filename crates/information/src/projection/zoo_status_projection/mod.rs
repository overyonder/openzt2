use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_zoo_state::progression::profile_challenge_types::TotalEndangeredAnimalBirthCount;
use openzt2_zoo_state::progression::rating_types::ZooRating;

#[expect(clippy::needless_pass_by_value)]
pub(crate) fn project_live_zoo_status_to_authored_information_bindings(
    cash: Option<Res<ZooCash>>,
    fame: Option<Res<Fame>>,
    rating: Option<Res<ZooRating>>,
    endangered_births: Option<Res<TotalEndangeredAnimalBirthCount>>,
    mut values: Query<(Ref<UiValueBinding>, &mut UiValue, &mut Visibility)>,
    history: Option<Res<openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory>>,
    mut guest_texts: Query<(
        Ref<openzt2_ui::authored_ui_text_content_binding::UiTextBinding>,
        &mut Text,
    )>,
    mut previous_guest_count: Local<Option<i32>>,
) {
    let guest_count = history
        .as_ref()
        .and_then(|history| history.current_month_finance())
        .map_or(0, |month| month.active_users);
    let guest_count_changed = *previous_guest_count != Some(guest_count);
    for (binding, mut text) in &mut guest_texts {
        if matches!(binding.0, UiTextPropertyBindingSource::ZooGuestCount { .. })
            && (guest_count_changed || binding.is_added())
        {
            super::text_replacement_operations::replace_projected_ui_text_if_changed(
                &mut text.0,
                &guest_count.to_string(),
            );
        }
    }
    *previous_guest_count = Some(guest_count);
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
            UiIntegerPropertyBindingSource::ZooCashCents => cash.as_ref().map(|cash| cash.0 .0),
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
        app.insert_resource(history).add_systems(
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
            .resource_mut::<MonthlyFinanceHistory>()
            .record_guest_arrival();
        app.update();
        assert_eq!(app.world().get::<Text>(field).unwrap().0, "1");
        app.world_mut()
            .resource_mut::<MonthlyFinanceHistory>()
            .record_guest_departure();
        app.update();
        assert_eq!(app.world().get::<Text>(field).unwrap().0, "0");
    }
}
