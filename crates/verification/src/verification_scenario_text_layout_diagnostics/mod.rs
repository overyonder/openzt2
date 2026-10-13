//! Layout of the challenge-offer paragraphs and goal-panel rows, logged at
//! debug level whenever it changes.

use std::collections::BTreeMap;

use bevy::{ecs::system::SystemParam, prelude::*, ui::CalculatedClip};
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;

use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_ui::authored_ui_text_layout_presentation::UiPhysicalFontSize;
use openzt2_ui::authored_ui_text_layout_presentation::UiSingleLineTextAlignment;

const OFFER_TEXT_NAMES: [&str; 2] = [
    "instant challenge text",
    "instant challenge description text",
];

#[derive(SystemParam)]
pub(super) struct ScenarioTextLayoutDiagnosticQueries<'w, 's> {
    nodes: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static Name>,
            &'static Node,
            Option<&'static ComputedNode>,
            Option<&'static UiGlobalTransform>,
            Option<&'static TextLayout>,
            Option<&'static CalculatedClip>,
            Option<&'static InheritedVisibility>,
            Has<UiPhysicalFontSize>,
            Has<UiSingleLineTextAlignment>,
            Option<&'static Text>,
        ),
    >,
    rows: Query<'w, 's, (Entity, &'static UiListRow)>,
    lists: Query<'w, 's, &'static UiListPolicy>,
    parents: Query<'w, 's, &'static ChildOf>,
}

/// Logs each diagnosed node's layout when it differs from the last line logged.
pub(super) fn log_changed_scenario_text_layout(
    queries: ScenarioTextLayoutDiagnosticQueries,
    mut logged: Local<BTreeMap<Entity, String>>,
) {
    if !bevy::log::tracing::enabled!(target: "crate::scenario_text_layout", bevy::log::Level::DEBUG)
    {
        return;
    }
    let goal_rows = queries
        .rows
        .iter()
        .filter(|(_, row)| {
            queries.lists.get(row.list).is_ok_and(|policy| {
                policy.source == UiWidgetLiveCollectionSource::ScenarioObjectives
            })
        })
        .map(|(entity, row)| (entity, row.index))
        .collect::<BTreeMap<_, _>>();
    let goal_row_of = |entity: Entity| {
        std::iter::successors(Some(entity), |entity| {
            queries.parents.get(*entity).ok().map(ChildOf::parent)
        })
        .find_map(|ancestor| goal_rows.get(&ancestor).copied())
    };
    let mut seen = Vec::new();
    for (
        entity,
        name,
        node,
        computed,
        transform,
        layout,
        clip,
        visibility,
        visual_projection,
        single_line,
        text,
    ) in &queries.nodes
    {
        let name = name.map_or("", Name::as_str);
        let row = goal_row_of(entity);
        let subject = if OFFER_TEXT_NAMES.contains(&name) {
            "offer text"
        } else if goal_rows.contains_key(&entity) {
            "goal row"
        } else if row.is_some() && name == "label" {
            "goal row label"
        } else {
            continue;
        };
        seen.push(entity);
        let line = format!(
            "size={:?} translation={:?} width={:?} height={:?} min_width={:?} max_width={:?} \
             display={:?} linebreak={:?} justify={:?} clip={:?} visible={:?} \
             visual_projection={visual_projection} single_line={single_line} text_chars={}",
            computed.map(ComputedNode::size),
            transform.map(|transform| transform.translation),
            node.width,
            node.height,
            node.min_width,
            node.max_width,
            node.display,
            layout.map(|layout| layout.linebreak),
            layout.map(|layout| layout.justify),
            clip.map(|clip| clip.clip),
            visibility.map(|visibility| visibility.get()),
            text.map_or(0, |text| text.0.chars().count()),
        );
        if logged.get(&entity) != Some(&line) {
            debug!(
                target: "crate::scenario_text_layout",
                subject, name, ?entity, row, "{line}"
            );
            logged.insert(entity, line);
        }
    }
    logged.retain(|entity, _| seen.contains(entity));
}
