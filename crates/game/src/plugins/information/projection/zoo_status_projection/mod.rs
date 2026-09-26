use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use crate::plugins::{
    economy::zoo_cash_types::ZooCash,
    progression::{
        fame_types::Fame, profile_challenge_types::TotalEndangeredAnimalBirthCount,
        rating_types::ZooRating,
    },
    ui::{
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
    },
};

pub(in crate::plugins::information) fn project_live_zoo_status_to_authored_information_bindings(
    cash: Option<Res<ZooCash>>,
    fame: Option<Res<Fame>>,
    rating: Option<Res<ZooRating>>,
    endangered_births: Option<Res<TotalEndangeredAnimalBirthCount>>,
    mut values: Query<(Ref<UiValueBinding>, &mut UiValue, &mut Visibility)>,
    history: Option<Res<crate::plugins::economy::monthly_finance_types::MonthlyFinanceHistory>>,
    mut guest_texts: Query<(
        Ref<crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding>,
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
    let (Some(cash), Some(fame), Some(rating), Some(endangered_births)) =
        (cash, fame, rating, endangered_births)
    else {
        return;
    };
    let facts_changed = guest_count_changed
        || cash.is_changed()
        || fame.is_changed()
        || rating.is_changed()
        || endangered_births.is_changed();
    for (binding, mut value, mut visibility) in &mut values {
        if !facts_changed && !binding.is_changed() {
            continue;
        }
        value.0 = match &binding.0 {
            UiIntegerPropertyBindingSource::ZooCashCents => cash.0 .0,
            UiIntegerPropertyBindingSource::ZooFameHalfStars => i64::from(fame.half_stars),
            UiIntegerPropertyBindingSource::ZooRatingPermille => {
                visibility.set_if_neq(if rating.overall_available {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if !rating.overall_available {
                    continue;
                }
                i64::from(rating.overall_permille)
            }
            UiIntegerPropertyBindingSource::ZooGuestCount => i64::from(guest_count),
            UiIntegerPropertyBindingSource::EndangeredBirthCount => i64::from(endangered_births.0),
            _ => continue,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{
        economy::{money_types::Money, monthly_finance_types::MonthlyFinanceHistory},
        ui::authored_ui_text_content_binding::UiTextBinding,
    };

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
