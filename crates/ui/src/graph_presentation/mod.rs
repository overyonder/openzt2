use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_interface_state::information::information_graph_types::InformationGraph;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::progression::fame_history_types::FameHistory;

use super::graph_generated_geometry::{
    GraphDrawings, PLOT_BOTTOM, PLOT_LEFT, PLOT_RIGHT, PLOT_TOP,
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

pub(super) fn update_visible_authored_graph_drawings(
    mut commands: Commands,
    finance: Single<Ref<MonthlyFinanceHistory>>,
    fame: Single<Ref<FameHistory>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    graphs: Query<(
        Entity,
        Ref<UiGraphPresentationPolicy>,
        Option<Ref<InformationGraph>>,
        Ref<ComputedNode>,
        Ref<InheritedVisibility>,
        Option<&Children>,
    )>,
    mut elements: Query<
        (
            &mut Node,
            &mut UiTransform,
            Option<&mut Text>,
            Option<&mut TextLayout>,
        ),
        With<UiGeneratedGraphPresentationElement>,
    >,
) {
    let finance = finance.into_inner();
    let fame = fame.into_inner();
    let localization_changed = active_localization.is_changed() || localizations.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (entity, policy, selection, computed, inherited, children) in &graphs {
        if !inherited.get() {
            continue;
        }
        let series = selection.as_ref().and_then(|selection| selection.graph);
        let history_changed = match series {
            Some(InformationGraphSeries::Fame) => fame.is_changed(),
            Some(InformationGraphSeries::Members) | None => false,
            _ => finance.is_changed(),
        };
        let update_plot = history_changed
            || policy.is_changed()
            || computed.is_changed()
            || inherited.is_changed()
            || selection
                .as_ref()
                .is_some_and(|selection| selection.is_changed());
        if !update_plot && !localization_changed {
            continue;
        }
        let mut drawings = GraphDrawings {
            plot: update_plot.then(Vec::new),
            labels: Some(Vec::new()),
        };
        let size = computed.size() * computed.inverse_scale_factor();
        if let Some(series) =
            series.filter(|_| size.x > PLOT_LEFT + PLOT_RIGHT && size.y > PLOT_TOP + PLOT_BOTTOM)
        {
            let graph_type = selection
                .as_ref()
                .and_then(|selection| selection.graph_type)
                .or(policy.graph_type)
                .unwrap_or(InformationGraphType::Line);
            render_selected_information_graph_series_from_canonical_histories(
                &mut drawings,
                *policy,
                graph_type,
                size,
                localization,
                series,
                &finance,
                &fame,
            );
        }

        let mut plot_entities = Vec::new();
        let mut label_entities = Vec::new();
        for child in children.into_iter().flatten().copied() {
            if let Ok((_, _, text, _)) = elements.get(child) {
                if text.is_some() {
                    label_entities.push(child);
                } else {
                    plot_entities.push(child);
                }
            }
        }
        if let Some(plot) = drawings.plot {
            let mut existing = plot_entities.into_iter();
            for (rectangle, transform) in plot {
                let layout = Node {
                    position_type: PositionType::Absolute,
                    left: px(rectangle.left),
                    bottom: px(rectangle.bottom),
                    width: px(rectangle.width),
                    height: px(rectangle.height),
                    ..default()
                };
                if let Some((mut node, mut current_transform, _, _)) = existing
                    .next()
                    .and_then(|child| elements.get_mut(child).ok())
                {
                    node.set_if_neq(layout);
                    current_transform.set_if_neq(transform);
                } else {
                    commands.spawn((
                        UiGeneratedGraphPresentationElement,
                        ChildOf(entity),
                        layout,
                        transform,
                        BackgroundColor(Color::srgb(0.72, 0.92, 0.31)),
                        Pickable::IGNORE,
                    ));
                }
            }
            for child in existing {
                commands.entity(child).despawn();
            }
        }
        if let Some(labels) = drawings.labels {
            let mut existing = label_entities.into_iter();
            for label in labels {
                let layout = Node {
                    position_type: PositionType::Absolute,
                    left: px(label.left),
                    bottom: px(label.bottom),
                    width: px(label.width),
                    ..default()
                };
                if let Some((mut node, _, Some(text), Some(text_layout))) = existing
                    .next()
                    .and_then(|child| elements.get_mut(child).ok())
                {
                    node.set_if_neq(layout);
                    text.map_unchanged(|text| &mut text.0)
                        .set_if_neq(label.text);
                    text_layout
                        .map_unchanged(|layout| &mut layout.justify)
                        .set_if_neq(label.justify);
                } else {
                    commands.spawn((
                        UiGeneratedGraphPresentationElement,
                        ChildOf(entity),
                        layout,
                        Text::new(label.text),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        TextLayout::justify(label.justify),
                        Pickable::IGNORE,
                    ));
                }
            }
            for child in existing {
                commands.entity(child).despawn();
            }
        }
    }
}
