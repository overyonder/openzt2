//! A guest's need icons in its panel's `guest needs` list, as `needsitem.xml`
//! rows like an animal's.

use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource,
    world_definitions::guest_simulation_definitions::{GuestDefinition, GuestNeedKind},
    AssetId,
};
use openzt2_interface_state::information::entity_selection_types::InfoPanel;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListPolicy, UiListRow,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentOwner, UiValue,
};
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;
use openzt2_ui::authored_multi_icon_presentation::UiMultiIconPolicy;
use openzt2_ui::authored_tooltip_presentation::{UiShortTooltipKey, UiTooltipStatusKey};
use openzt2_zoo_state::guests::guest_simulation_types::{
    Guest, GuestAmusement, GuestArchetype, GuestEnergy, GuestHunger, GuestRestroom,
    GuestSatisfaction, GuestThirst,
};

use super::selected_animal_need_list_projection::{
    set_need_icon_fill, set_tooltip_status, status_label, CRITICAL_COLOR, PRESSING_COLOR,
    SATISFIED_COLOR,
};

/// The rows of a guest's need list: its happiness, then the needs the
/// original's needs labels name for guests.
const GUEST_NEED_ROWS: [GuestNeedRow; 6] = [
    GuestNeedRow::Happiness,
    GuestNeedRow::Need(GuestNeedKind::Hunger),
    GuestNeedRow::Need(GuestNeedKind::Thirst),
    GuestNeedRow::Need(GuestNeedKind::Energy),
    GuestNeedRow::Need(GuestNeedKind::Restroom),
    GuestNeedRow::Need(GuestNeedKind::Amusement),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GuestNeedRow {
    Happiness,
    Need(GuestNeedKind),
}

/// `needsitem.xml` icon index and `needs.xml` label of a row.
fn guest_need_icon_and_label(row: GuestNeedRow, unhappy: bool) -> (i64, &'static str) {
    match row {
        GuestNeedRow::Happiness if unhappy => (17, "entityneeds:guest_happy"),
        GuestNeedRow::Happiness => (16, "entityneeds:guest_happy"),
        GuestNeedRow::Need(GuestNeedKind::Hunger) => (18, "entityneeds:guest_hunger"),
        GuestNeedRow::Need(GuestNeedKind::Thirst) => (19, "entityneeds:guest_thirst"),
        GuestNeedRow::Need(GuestNeedKind::Energy) => (20, "entityneeds:guest_rest"),
        GuestNeedRow::Need(GuestNeedKind::Amusement) => (21, "entityneeds:guest_amusement"),
        GuestNeedRow::Need(_) => (22, "entityneeds:guest_bathroom"),
    }
}

/// A need's deprivation in permille and its colour band: pressing from the
/// guest's reconsider threshold, critical past its critical one.
fn guest_need_band(definition: &GuestDefinition, kind: GuestNeedKind, wellness: u16) -> Color {
    let deprivation = 1_000_u16.saturating_sub(wellness.min(1_000));
    let Some(need) = definition.needs.iter().find(|need| need.kind == kind) else {
        return SATISFIED_COLOR;
    };
    if i64::from(deprivation) << 16 > i64::from(need.critical_q16) * 10 {
        CRITICAL_COLOR
    } else if deprivation >= need.reconsider_threshold {
        PRESSING_COLOR
    } else {
        SATISFIED_COLOR
    }
}

pub(crate) fn request_selected_guest_need_row_counts(
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner)>,
    panels: Query<&InfoPanel>,
    guests: Query<(), With<Guest>>,
    mut requested: Local<bevy::platform::collections::HashMap<Entity, u16>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    requested.retain(|list, _| lists.contains(*list));
    for (list, policy, owner) in &lists {
        if policy.source != UiWidgetLiveCollectionSource::SelectedGuestNeeds {
            continue;
        }
        let count = panels
            .get(owner.0)
            .ok()
            .filter(|panel| guests.contains(panel.subject))
            .map_or(0, |_| GUEST_NEED_ROWS.len() as u16);
        if requested.insert(list, count) != Some(count) {
            row_count_requests.write(SetUiListRowCount { list, count });
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn project_selected_guest_needs_to_list_rows(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    panels: Query<&InfoPanel>,
    guests: Query<
        (
            &GuestArchetype,
            &GuestSatisfaction,
            &GuestHunger,
            &GuestThirst,
            &GuestEnergy,
            &GuestRestroom,
            &GuestAmusement,
        ),
        With<Guest>,
    >,
    rows: Query<(
        Entity,
        &UiListRow,
        Option<&UiShortTooltipKey>,
        Option<&UiTooltipStatusKey>,
    )>,
    mut commands: Commands,
    mut multi_icons: Query<(&UiDocumentOwner, &mut UiValue), With<UiMultiIconPolicy>>,
    mut foreground_layouts: Query<(Entity, &Name, &UiDocumentOwner, &mut Node)>,
    mut foreground_visuals: Query<(&ChildOf, &mut ImageNode), With<UiVisualLayer>>,
) {
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (row_entity, row, tooltip, status) in &rows {
        let Ok((policy, owner)) = lists.get(row.list) else {
            continue;
        };
        if policy.source != UiWidgetLiveCollectionSource::SelectedGuestNeeds {
            continue;
        }
        let Some(need_row) = GUEST_NEED_ROWS.get(usize::from(row.index)).copied() else {
            continue;
        };
        let Ok((archetype, satisfaction, hunger, thirst, energy, restroom, amusement)) = panels
            .get(owner.0)
            .and_then(|panel| guests.get(panel.subject).map_err(Into::into))
        else {
            continue;
        };
        let Some(definition) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let (wellness, color) = match need_row {
            GuestNeedRow::Happiness => {
                let deprivation_q16 = satisfaction.source_q16();
                let policy = definition.happiness_need;
                let color = if policy
                    .and_then(|policy| policy.critical_q16)
                    .is_some_and(|threshold| deprivation_q16 > threshold)
                {
                    CRITICAL_COLOR
                } else if policy
                    .and_then(|policy| policy.trigger_q16)
                    .is_some_and(|threshold| deprivation_q16 > threshold)
                {
                    PRESSING_COLOR
                } else {
                    SATISFIED_COLOR
                };
                (satisfaction.0.min(1_000), color)
            }
            GuestNeedRow::Need(kind) => {
                let wellness = match kind {
                    GuestNeedKind::Hunger => hunger.value,
                    GuestNeedKind::Thirst => thirst.value,
                    GuestNeedKind::Energy => energy.value,
                    GuestNeedKind::Restroom => restroom.value,
                    _ => amusement.value,
                };
                (wellness, guest_need_band(definition, kind, wellness))
            }
        };
        let (icon, label) = guest_need_icon_and_label(need_row, color != SATISFIED_COLOR);
        let label = AssetId::from_key(label);
        if tooltip.map(|tooltip| tooltip.0) != Some(label) {
            // A plain icon item takes no hits; its tooltip needs the pointer.
            commands.entity(row_entity).insert((
                UiShortTooltipKey(label),
                Pickable::default(),
                UiPointerInteraction::default(),
            ));
        }
        set_tooltip_status(&mut commands, row_entity, status, Some(status_label(color)));
        for (icon_owner, mut value) in &mut multi_icons {
            if icon_owner.0 == row_entity && value.0 != icon {
                value.0 = icon;
            }
        }
        set_need_icon_fill(
            row_entity,
            100.0 - f32::from(wellness.min(1_000)) / 10.0,
            color,
            &mut foreground_layouts,
            &mut foreground_visuals,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_guest_need_row_has_its_own_icon_and_label() {
        let icons: std::collections::HashSet<_> = GUEST_NEED_ROWS
            .iter()
            .map(|row| guest_need_icon_and_label(*row, false))
            .collect();
        assert_eq!(icons.len(), GUEST_NEED_ROWS.len());
        assert_eq!(
            guest_need_icon_and_label(GuestNeedRow::Happiness, true).0,
            17
        );
    }
}
