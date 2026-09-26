use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType,
};

use crate::{
    assets::localization::loaded_localization_queries::LoadedLocalizationView,
    plugins::{
        economy::monthly_finance_types::{FinanceMetric, MonthlyFinance, MonthlyFinanceHistory},
        progression::fame_history_types::FameHistory,
    },
};

use super::{
    graph_generated_geometry::{
        spawn_colored_graph_geometry_element, spawn_graph_line_segment, spawn_graph_text_label,
        spawn_localized_currency_graph_y_axis_labels, spawn_numeric_graph_y_axis_labels,
        GeneratedGraphElementRectangle, PLOT_BOTTOM, PLOT_LEFT, PLOT_RIGHT, PLOT_TOP,
    },
    graph_presentation_types::UiGraphPresentationPolicy,
};

#[derive(Clone, Copy)]
struct GraphHistorySeriesRenderingContext<'a> {
    parent: Entity,
    policy: UiGraphPresentationPolicy,
    graph_type: InformationGraphType,
    size: Vec2,
    localization: LoadedLocalizationView<'a>,
}

pub(super) fn render_selected_information_graph_series_from_canonical_histories(
    commands: &mut Commands,
    parent: Entity,
    policy: UiGraphPresentationPolicy,
    graph_type: InformationGraphType,
    size: Vec2,
    localization: LoadedLocalizationView<'_>,
    series: InformationGraphSeries,
    finance_history: &MonthlyFinanceHistory,
    fame_history: &FameHistory,
) {
    let graph = GraphHistorySeriesRenderingContext {
        parent,
        policy,
        graph_type,
        size,
        localization,
    };
    let retained_sample_limit = usize::from(policy.x_labels);
    match series {
        InformationGraphSeries::ZooValue => render_monthly_finance_history_series(
            commands,
            graph,
            finance_history,
            FinanceMetric::Cash,
            retained_sample_limit,
        ),
        InformationGraphSeries::ZooProfit => render_monthly_finance_history_series(
            commands,
            graph,
            finance_history,
            FinanceMetric::Profit,
            retained_sample_limit,
        ),
        InformationGraphSeries::DonationIncome => render_monthly_finance_history_series(
            commands,
            graph,
            finance_history,
            FinanceMetric::DonationIncome,
            retained_sample_limit,
        ),
        InformationGraphSeries::AdmissionUsers => render_monthly_finance_history_series(
            commands,
            graph,
            finance_history,
            FinanceMetric::TotalUsers,
            retained_sample_limit,
        ),
        InformationGraphSeries::Fame => {
            let samples = fame_history.samples();
            let samples = &samples[samples.len().saturating_sub(retained_sample_limit)..];
            render_graph_history_samples(
                commands,
                graph,
                || {
                    samples
                        .iter()
                        .map(|sample| (sample.month_index, i64::from(sample.half_stars)))
                },
                false,
            );
        }
        // The shipped graph selector contains this legacy spelling, but the
        // shipped game supplies no corresponding monthly series.
        InformationGraphSeries::Members => {}
    }
}

fn render_monthly_finance_history_series(
    commands: &mut Commands,
    graph: GraphHistorySeriesRenderingContext<'_>,
    history: &MonthlyFinanceHistory,
    metric: FinanceMetric,
    retained_sample_limit: usize,
) {
    let skip = history
        .retained_month_finance_records()
        .len()
        .saturating_sub(retained_sample_limit);
    render_graph_history_samples(
        commands,
        graph,
        || {
            history
                .retained_month_finance_records()
                .skip(skip)
                .map(|sample| {
                    (
                        sample.month_ordinal,
                        monthly_finance_metric_value(sample, metric),
                    )
                })
        },
        !matches!(metric, FinanceMetric::TotalUsers),
    );
}

fn monthly_finance_metric_value(sample: &MonthlyFinance, metric: FinanceMetric) -> i64 {
    match metric {
        FinanceMetric::Cash => sample.closing_cash.0,
        FinanceMetric::Income => sample.income.0,
        FinanceMetric::Expenses => sample.expenses.0,
        FinanceMetric::Profit => sample.income.0.saturating_sub(sample.expenses.0),
        FinanceMetric::DonationIncome => sample.donation_income.0,
        FinanceMetric::TotalUsers => i64::from(sample.total_users),
    }
}

// The canonical histories are bounded to 120 entries and UI geometry is f32;
// these conversions cannot lose a player-visible sample position.
#[allow(clippy::cast_precision_loss)]
fn render_graph_history_samples<I>(
    commands: &mut Commands,
    graph: GraphHistorySeriesRenderingContext<'_>,
    samples: impl Fn() -> I,
    values_are_currency: bool,
) where
    I: ExactSizeIterator<Item = (u32, i64)>,
{
    let count = samples().len();
    if count == 0 {
        return;
    }
    let (minimum, maximum) = samples().fold((0_i64, 0_i64), |(minimum, maximum), (_, value)| {
        (minimum.min(value), maximum.max(value))
    });
    let y_axis =
        GraphYAxisRange::from_policy_and_sample_range(graph.policy, minimum as f64, maximum as f64);
    let plot_width = graph.size.x - PLOT_LEFT - PLOT_RIGHT;
    let plot_height = graph.size.y - PLOT_TOP - PLOT_BOTTOM;
    let x_step = if count <= 1 {
        0.0
    } else {
        plot_width / (count - 1) as f32
    };
    let slot_width = plot_width / count as f32;
    let baseline = graph_value_bottom_position(y_axis, 0.0, plot_height);
    let mut previous = None;

    for (index, (month, value)) in samples().enumerate() {
        let x = PLOT_LEFT
            + if count <= 1 {
                plot_width * 0.5
            } else {
                index as f32 * x_step
            };
        let y = graph_value_bottom_position(y_axis, value as f64, plot_height);
        match graph.graph_type {
            InformationGraphType::Bar => {
                let width = (slot_width * 0.7).max(1.0);
                spawn_colored_graph_geometry_element(
                    commands,
                    graph.parent,
                    GeneratedGraphElementRectangle {
                        left: x - width * 0.5,
                        bottom: y.min(baseline),
                        width,
                        height: (y - baseline).abs().max(1.0),
                    },
                    UiTransform::IDENTITY,
                );
            }
            InformationGraphType::Line => {
                if let Some((previous_x, previous_y)) = previous {
                    spawn_graph_line_segment(commands, graph.parent, previous_x, previous_y, x, y);
                }
                spawn_colored_graph_geometry_element(
                    commands,
                    graph.parent,
                    GeneratedGraphElementRectangle {
                        left: x - 2.0,
                        bottom: y - 2.0,
                        width: 4.0,
                        height: 4.0,
                    },
                    UiTransform::IDENTITY,
                );
                previous = Some((x, y));
            }
        }
        if let Some(label) = graph
            .localization
            .localized_month_abbreviation(u8::try_from(month % 12 + 1).unwrap_or(1))
        {
            spawn_graph_text_label(
                commands,
                graph.parent,
                label,
                x - slot_width * 0.5,
                2.0,
                slot_width,
                Justify::Center,
            );
        }
    }

    if values_are_currency {
        spawn_localized_currency_graph_y_axis_labels(
            commands,
            graph.parent,
            graph.policy.y_labels,
            y_axis.minimum,
            y_axis.maximum,
            plot_height,
            graph.localization,
        );
    } else {
        spawn_numeric_graph_y_axis_labels(
            commands,
            graph.parent,
            graph.policy.y_labels,
            y_axis.minimum,
            y_axis.maximum,
            plot_height,
        );
    }
}

#[derive(Clone, Copy)]
struct GraphYAxisRange {
    minimum: f64,
    maximum: f64,
}

impl GraphYAxisRange {
    fn from_policy_and_sample_range(
        policy: UiGraphPresentationPolicy,
        minimum: f64,
        maximum: f64,
    ) -> Self {
        if policy.force_y_values {
            let minimum = f64::from(policy.forced_minimum_y);
            let maximum = f64::from(policy.forced_maximum_y);
            return if minimum < maximum {
                Self { minimum, maximum }
            } else {
                Self {
                    minimum: maximum,
                    maximum: minimum.max(maximum + 1.0),
                }
            };
        }
        let intervals = f64::from(policy.y_labels.saturating_sub(1));
        let span = (maximum - minimum).max(1.0);
        let step = choose_human_readable_graph_y_axis_step(span / intervals);
        let minimum = (minimum / step).floor() * step;
        let maximum = (maximum / step).ceil() * step;
        Self {
            minimum,
            maximum: maximum.max(minimum + step),
        }
    }
}

fn choose_human_readable_graph_y_axis_step(value: f64) -> f64 {
    let magnitude = 10.0_f64.powf(value.log10().floor());
    let normalized = value / magnitude;
    let factor = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    factor * magnitude
}

// The result is deliberately reduced to Bevy's f32 UI coordinate space.
#[allow(clippy::cast_possible_truncation)]
fn graph_value_bottom_position(y_axis: GraphYAxisRange, value: f64, plot_height: f32) -> f32 {
    let ratio = ((value - y_axis.minimum) / (y_axis.maximum - y_axis.minimum)).clamp(0.0, 1.0);
    PLOT_BOTTOM + ratio as f32 * plot_height
}
