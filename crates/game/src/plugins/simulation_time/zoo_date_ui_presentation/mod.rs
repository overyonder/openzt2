use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::ui::authored_ui_text_content_binding::UiTextBinding,
};

use super::simulation_clock_types::ZooCalendar;

/// Updates the date field when the calendar changes, reusing its string allocation.
pub(super) fn project_authoritative_zoo_calendar_into_authored_zoo_date_text(
    zoo_calendar: Res<ZooCalendar>,
    active_localization_assets: Option<Res<LocalizationPrecedenceIndex>>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut ui_text_bindings: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Some(active_localization) = active_localization_assets
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localization_assets))
    else {
        return;
    };
    for (ui_text_binding, mut text) in &mut ui_text_bindings {
        if !matches!(ui_text_binding.0, UiTextPropertyBindingSource::ZooDate)
            || (!zoo_calendar.is_changed() && !ui_text_binding.is_added())
        {
            continue;
        }
        text.0.clear();
        let _ = active_localization.write_localized_month_and_year(
            zoo_calendar.month,
            zoo_calendar.year,
            &mut text.0,
        );
    }
}
