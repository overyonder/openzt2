use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::InformationGraphType;

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::{
        economy::monthly_finance_types::MonthlyFinanceHistory,
        information::information_graph_types::InformationGraph,
        progression::fame_history_types::FameHistory,
    },
};

use super::graph_generated_geometry::{
    despawn_generated_graph_presentation_elements, PLOT_BOTTOM, PLOT_LEFT, PLOT_RIGHT, PLOT_TOP,
};
use super::graph_history_series_rendering::render_selected_information_graph_series_from_canonical_histories;
use super::graph_presentation_types::{
    UiGeneratedGraphPresentationElement, UiGraphPresentationPolicy,
};

pub(super) fn clip_authored_graph_generated_geometry(
    mut graphs: Query<&mut Node, Added<UiGraphPresentationPolicy>>,
) {
    for mut graph in &mut graphs {
        graph.overflow = Overflow::clip();
    }
}

pub(super) fn regenerate_visible_authored_graph_presentations_from_canonical_histories(
    mut commands: Commands,
    finance: Res<MonthlyFinanceHistory>,
    fame: Res<FameHistory>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    graphs: Query<(
        Entity,
        Ref<UiGraphPresentationPolicy>,
        Option<Ref<InformationGraph>>,
        Ref<ComputedNode>,
        Ref<InheritedVisibility>,
    )>,
    elements: Query<(Entity, &ChildOf), With<UiGeneratedGraphPresentationElement>>,
) {
    let localization_changed = active_localization.is_changed() || localizations.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };

    for (entity, policy, selection, computed, inherited) in &graphs {
        let selection_changed = selection.as_ref().is_some_and(|value| value.is_changed());
        if !finance.is_changed()
            && !fame.is_changed()
            && !localization_changed
            && !policy.is_changed()
            && !selection_changed
            && !computed.is_changed()
            && !inherited.is_changed()
        {
            continue;
        }

        despawn_generated_graph_presentation_elements(&mut commands, entity, &elements);
        if !inherited.get() {
            continue;
        }
        let Some(selection) = selection else {
            continue;
        };
        let Some(series) = selection.graph else {
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        if size.x <= PLOT_LEFT + PLOT_RIGHT || size.y <= PLOT_TOP + PLOT_BOTTOM {
            continue;
        }

        let graph_type = selection
            .graph_type
            .or(policy.graph_type)
            .unwrap_or(InformationGraphType::Line);
        render_selected_information_graph_series_from_canonical_histories(
            &mut commands,
            entity,
            *policy,
            graph_type,
            size,
            localization,
            series,
            &finance,
            &fame,
        );
    }
}
