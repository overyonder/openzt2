use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};

use super::simulation_clock_types::{UiZooDateText, ZooCalendar};

pub(super) fn zoo_date_inputs_changed(
    calendars: Query<(), (With<WorldRoot>, Changed<ZooCalendar>)>,
    dates: Query<
        (),
        (
            With<UiZooDateText>,
            With<Text>,
            Or<(Changed<UiZooDateText>, Added<Text>)>,
        ),
    >,
    localization: Option<Res<LocalizationPrecedenceIndex>>,
) -> bool {
    !calendars.is_empty()
        || !dates.is_empty()
        || localization.is_some_and(|index| index.is_changed())
}

pub(super) fn project_localized_zoo_date(
    roots: Query<Ref<ZooCalendar>, With<WorldRoot>>,
    localization: Option<Res<LocalizationPrecedenceIndex>>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut dates: Query<(Ref<UiZooDateText>, &mut Text)>,
) {
    let (Ok(calendar), Some(localization)) = (roots.single(), localization) else {
        return;
    };
    let refresh_all = calendar.is_changed() || localization.is_changed();
    let Some(view) = localization.borrow_loaded_localization_view(&localization_assets) else {
        return;
    };
    for (date, mut text) in &mut dates {
        if refresh_all || date.is_changed() || text.is_added() {
            text.0.clear();
            let _ = view.write_localized_month_and_year(calendar.month, calendar.year, &mut text.0);
        }
    }
}
