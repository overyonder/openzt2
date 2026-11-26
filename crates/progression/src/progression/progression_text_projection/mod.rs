use bevy::prelude::*;
use openzt2_game_data::localization::LocalizationFormatArgument;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};

use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::{fame_types::Fame, rating_types::ZooRating, research_types::ResearchProject};

/// Projects canonical progression facts through authored locale formats.
pub(super) fn project_progression_facts_into_authored_ui_text(
    time: Res<Time<Fixed>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    worlds: Query<
        (
            Ref<Fame>,
            Ref<ZooRating>,
            Ref<super::ResearchPresentationProject>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    projects: Query<(
        Ref<ResearchProject>,
        Option<Ref<super::ResearchCompletionBasisPoints>>,
    )>,
    mut removed_projects: RemovedComponents<ResearchProject>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let world = worlds.single().ok();
    let project = world
        .as_ref()
        .and_then(|(_, _, selection)| projects.get(selection.0?).ok());
    let project_removed = removed_projects.read().next().is_some();
    let localization_changed = active_localization.is_changed() || localizations.is_changed();

    for (binding, mut text) in &mut fields {
        let source_changed = match &binding.0 {
            UiTextPropertyBindingSource::ZooFame { .. } => {
                world.as_ref().is_some_and(|(fame, _, _)| fame.is_changed())
            }
            UiTextPropertyBindingSource::ZooRating { .. } => world
                .as_ref()
                .is_some_and(|(_, rating, _)| rating.is_changed()),
            UiTextPropertyBindingSource::ResearchProgress { .. } => {
                project_removed
                    || world
                        .as_ref()
                        .is_some_and(|(_, _, selection)| selection.is_changed())
                    || project.as_ref().is_some_and(|(project, progress)| {
                        project.is_changed() || progress.as_ref().is_some_and(Ref::is_changed)
                    })
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
                        world.as_ref().map_or(0, |(fame, _, _)| fame.half_stars),
                    ))],
                    &mut text.0,
                );
            }
            UiTextPropertyBindingSource::ZooRating { format } => {
                let Some((_, rating, _)) = world
                    .as_ref()
                    .filter(|(_, rating, _)| rating.overall_available)
                else {
                    continue;
                };
                let _ = localization.write_localized_text_with_format_arguments(
                    *format,
                    &[LocalizationFormatArgument::PercentBasisPoints(
                        i64::from(rating.overall_permille) * 10,
                    )],
                    &mut text.0,
                );
            }
            UiTextPropertyBindingSource::ResearchProgress { format } => {
                let (required, elapsed) = project
                    .as_ref()
                    .map(|(project, _)| project.saved_ticks(time.timestep()))
                    .unwrap_or_default();
                let percentage_basis_points = project
                    .as_ref()
                    .and_then(|(_, progress)| progress.as_ref())
                    .map_or(0, |progress| progress.0);
                let arguments = [
                    LocalizationFormatArgument::Integer(i64::try_from(elapsed).unwrap_or(i64::MAX)),
                    LocalizationFormatArgument::Integer(
                        i64::try_from(required).unwrap_or(i64::MAX),
                    ),
                    LocalizationFormatArgument::PercentBasisPoints(percentage_basis_points),
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
