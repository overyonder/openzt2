use bevy::prelude::*;
use openzt2_game_data::localization::LocalizationFormatArgument;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::ui::authored_ui_text_content_binding::UiTextBinding,
};

use super::{fame_types::Fame, rating_types::ZooRating, research_types::ResearchProject};

/// Projects canonical progression facts through authored locale formats.
pub(super) fn project_progression_facts_into_authored_ui_text(
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    fame: Res<Fame>,
    rating: Res<ZooRating>,
    projects: Query<Ref<ResearchProject>>,
    mut removed_projects: RemovedComponents<ResearchProject>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let project = projects.iter().next();
    let project_removed = removed_projects.read().next().is_some();
    let localization_changed = active_localization.is_changed() || localizations.is_changed();

    for (binding, mut text) in &mut fields {
        let source_changed = match &binding.0 {
            UiTextPropertyBindingSource::ZooFame { .. } => fame.is_changed(),
            UiTextPropertyBindingSource::ZooRating { .. } => rating.is_changed(),
            UiTextPropertyBindingSource::ResearchProgress { .. } => {
                project_removed || project.as_ref().is_some_and(|project| project.is_changed())
            }
            _ => continue,
        };
        if !binding.is_added() && !localization_changed && !source_changed {
            continue;
        }

        text.0.clear();
        match &binding.0 {
            UiTextPropertyBindingSource::ZooFame { format } => {
                let _ = localization.write_localized_text_with_format_arguments(
                    *format,
                    &[LocalizationFormatArgument::Integer(i64::from(
                        fame.half_stars,
                    ))],
                    &mut text.0,
                );
            }
            UiTextPropertyBindingSource::ZooRating { format } => {
                if !rating.overall_available {
                    continue;
                }
                let _ = localization.write_localized_text_with_format_arguments(
                    *format,
                    &[LocalizationFormatArgument::PercentBasisPoints(
                        i64::from(rating.overall_permille) * 10,
                    )],
                    &mut text.0,
                );
            }
            UiTextPropertyBindingSource::ResearchProgress { format } => {
                let (elapsed, required) = project
                    .as_ref()
                    .map(|project| (project.elapsed_ticks, project.required_ticks))
                    .unwrap_or_default();
                let percentage_basis_points = if required == 0 {
                    0
                } else {
                    u64::try_from(u128::from(elapsed.min(required)) * 10_000 / u128::from(required))
                        .unwrap_or(10_000)
                };
                let arguments = [
                    LocalizationFormatArgument::Integer(i64::try_from(elapsed).unwrap_or(i64::MAX)),
                    LocalizationFormatArgument::Integer(
                        i64::try_from(required).unwrap_or(i64::MAX),
                    ),
                    LocalizationFormatArgument::PercentBasisPoints(
                        i64::try_from(percentage_basis_points).unwrap_or(i64::MAX),
                    ),
                ];
                let _ = localization.write_localized_text_with_format_arguments(
                    *format,
                    &arguments,
                    &mut text.0,
                );
            }
            _ => unreachable!("binding source was filtered above"),
        }
    }
}
