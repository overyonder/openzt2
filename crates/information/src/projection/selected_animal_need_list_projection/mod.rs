use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::{
    species::NeedKind, ui_document::widget_live_collection::UiWidgetLiveCollectionSource,
};

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentMembers;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_ui::authored_multi_icon_presentation::UiMultiIconPolicy;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_welfare::types::EnvironmentNeed;
use openzt2_zoo_state::animal_welfare::types::ExerciseNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::PrivacyNeed;
use openzt2_zoo_state::animal_welfare::types::RestNeed;
use openzt2_zoo_state::animal_welfare::types::SocialNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;

use openzt2_interface_state::information::entity_selection_types::{
    InfoPanel, SelectedSubject, SelectionChanged,
};

const BASIC_ANIMAL_NEED_ROW_COUNT: u16 = 5;
const ADVANCED_ANIMAL_NEED_ROW_COUNT: u16 = 4;

pub(crate) fn request_selected_animal_need_list_row_counts(
    lists: Query<(Entity, Ref<UiListPolicy>, &UiDocumentOwner)>,
    information_panels: Query<&ChildOf, With<InfoPanel>>,
    selections: Query<Ref<SelectedSubject>>,
    mut selection_changes: MessageReader<SelectionChanged>,
    animals: Query<(), With<Animal>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    let selection_changed = selection_changes.read().count() != 0;
    for (list_entity, list_policy, document_owner) in &lists {
        let row_count = match list_policy.source {
            UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds => BASIC_ANIMAL_NEED_ROW_COUNT,
            UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds => {
                ADVANCED_ANIMAL_NEED_ROW_COUNT
            }
            _ => continue,
        };
        let Ok(parent) = information_panels.get(document_owner.0) else {
            continue;
        };
        let selection = selections.get(parent.parent()).ok();
        if !list_policy.is_changed()
            && !selection_changed
            && !selection.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        row_count_requests.write(SetUiListRowCount {
            list: list_entity,
            count: if selection.is_some_and(|selection| animals.contains(selection.0)) {
                row_count
            } else {
                0
            },
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn project_selected_animal_needs_to_authored_list_rows(
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    lists: Query<(Ref<UiListPolicy>, &UiDocumentOwner)>,
    information_panels: Query<(&ChildOf, Ref<InheritedVisibility>), With<InfoPanel>>,
    selections: Query<Ref<SelectedSubject>>,
    animals: Query<
        (
            &SpeciesHandle,
            &EnvironmentNeed,
            &Hunger,
            &Thirst,
            &RestNeed,
            &ExerciseNeed,
            &PrivacyNeed,
            &HygieneNeed,
            &SocialNeed,
            &StimulationNeed,
        ),
        With<Animal>,
    >,
    changed_needs: Query<
        (),
        Or<(
            Changed<SpeciesHandle>,
            Changed<EnvironmentNeed>,
            Changed<Hunger>,
            Changed<Thirst>,
            Changed<RestNeed>,
            Changed<ExerciseNeed>,
            Changed<PrivacyNeed>,
            Changed<HygieneNeed>,
            Changed<SocialNeed>,
            Changed<StimulationNeed>,
        )>,
    >,
    rows: Query<(Ref<UiListRow>, &ChildOf, Ref<UiDocumentMembers>)>,
    mut multi_icons: Query<&mut UiValue, With<UiMultiIconPolicy>>,
    mut foreground_layouts: Query<(&Name, &mut Node, Option<&Children>)>,
    mut foreground_visuals: Query<&mut ImageNode, With<UiVisualLayer>>,
) {
    let Some(species_view) = species_index.get(&species_assets) else {
        return;
    };
    let species_changed = species_assets.is_changed() || species_index.is_changed();
    for (row, parent, members) in &rows {
        let Ok((list_policy, document_owner)) = lists.get(parent.parent()) else {
            continue;
        };
        if !matches!(
            list_policy.source,
            UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds
                | UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds
        ) {
            continue;
        }
        let Ok((panel_parent, visible)) = information_panels.get(document_owner.0) else {
            continue;
        };
        if !visible.get() {
            continue;
        }
        let selection = selections.get(panel_parent.parent()).ok();
        let facts = selection
            .as_ref()
            .and_then(|selection| animals.get(selection.0).ok());
        if facts.is_some()
            && !species_changed
            && !list_policy.is_changed()
            && !visible.is_changed()
            && !row.is_changed()
            && !members.is_changed()
            && !selection.as_ref().is_some_and(|selection| {
                selection.is_changed() || changed_needs.contains(selection.0)
            })
        {
            continue;
        }
        let value = facts.and_then(|needs| {
            let (kind, icon, wellness) =
                selected_animal_need_row_value(list_policy.source, row.index, needs)?;
            let definition = species_view
                .find(needs.0.species)?
                .needs
                .iter()
                .find(|need| need.kind == kind)?;
            let wellness =
                u16::try_from(wellness.clamp(0, MAX_NEED_Q16) / Q16_ONE).unwrap_or_default();
            let color = if wellness <= definition.critical_wellness_threshold.unwrap_or_default() {
                Color::srgb(1.0, 0.0, 20.0 / 255.0)
            } else if wellness <= definition.pressing_wellness_threshold.unwrap_or_default() {
                Color::srgb(1.0, 1.0, 0.0)
            } else {
                Color::srgb(0.0, 1.0, 0.0)
            };
            Some((icon, 100.0 - f32::from(wellness) / 10.0, color))
        });
        let (icon, height, color) = value.unwrap_or((-1, 0.0, Color::NONE));
        for entity in members.iter() {
            if let Ok(mut value) = multi_icons.get_mut(entity) {
                value.set_if_neq(UiValue(icon));
            }
            let Ok((name, mut node, children)) = foreground_layouts.get_mut(entity) else {
                continue;
            };
            if !name.as_str().eq_ignore_ascii_case("icon foreground") {
                continue;
            }
            let next_height = Val::Percent(height);
            if node.height != next_height {
                node.height = next_height;
            }
            for child in children.into_iter().flat_map(RelationshipTarget::iter) {
                if let Ok(mut image) = foreground_visuals.get_mut(child) {
                    if image.color != color {
                        image.color = color;
                    }
                }
            }
        }
    }
}

type SelectedAnimalNeedComponents<'a> = (
    &'a SpeciesHandle,
    &'a EnvironmentNeed,
    &'a Hunger,
    &'a Thirst,
    &'a RestNeed,
    &'a ExerciseNeed,
    &'a PrivacyNeed,
    &'a HygieneNeed,
    &'a SocialNeed,
    &'a StimulationNeed,
);

fn selected_animal_need_row_value(
    source: UiWidgetLiveCollectionSource,
    row_index: u16,
    needs: SelectedAnimalNeedComponents<'_>,
) -> Option<(NeedKind, i64, i32)> {
    match (source, row_index) {
        (UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds, 0) => {
            Some((NeedKind::Environment, 2, needs.1 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds, 1) => {
            Some((NeedKind::Hunger, 0, needs.2 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds, 2) => {
            Some((NeedKind::Thirst, 1, needs.3 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds, 3) => {
            Some((NeedKind::Rest, 3, needs.4 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds, 4) => {
            Some((NeedKind::Exercise, 4, needs.5 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds, 0) => {
            Some((NeedKind::Privacy, 6, needs.6 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds, 1) => {
            Some((NeedKind::Hygiene, 7, needs.7 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds, 2) => {
            Some((NeedKind::Social, 8, needs.8 .0))
        }
        (UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds, 3) => {
            Some((NeedKind::Stimulation, 9, needs.9 .0))
        }
        _ => None,
    }
}
