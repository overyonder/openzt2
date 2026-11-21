use bevy::prelude::*;
use openzt2_game_data::{
    species::NeedKind, ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};
use openzt2_ui::authored_tooltip_presentation::UiShortTooltipKey;

use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
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

use openzt2_interface_state::information::entity_selection_types::InfoPanel;

const BASIC_ANIMAL_NEED_ROW_COUNT: u16 = 5;
const ADVANCED_ANIMAL_NEED_ROW_COUNT: u16 = 4;

pub(crate) fn request_selected_animal_need_list_row_counts(
    lists: Query<(Entity, Ref<UiListPolicy>, &UiDocumentOwner)>,
    information_panels: Query<Ref<InfoPanel>>,
    animals: Query<(), With<Animal>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    for (list_entity, list_policy, document_owner) in &lists {
        let row_count = match list_policy.source {
            UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds => BASIC_ANIMAL_NEED_ROW_COUNT,
            UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds => {
                ADVANCED_ANIMAL_NEED_ROW_COUNT
            }
            _ => continue,
        };
        let Ok(information_panel) = information_panels.get(document_owner.0) else {
            continue;
        };
        if !list_policy.is_added() && !information_panel.is_changed() {
            continue;
        }
        row_count_requests.write(SetUiListRowCount {
            list: list_entity,
            count: if animals.get(information_panel.subject).is_ok() {
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
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    information_panels: Query<&InfoPanel>,
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
    rows: Query<(Entity, &UiListRow, Option<&UiShortTooltipKey>)>,
    mut commands: Commands,
    mut multi_icons: Query<(&UiDocumentOwner, &mut UiValue), With<UiMultiIconPolicy>>,
    mut foreground_layouts: Query<(Entity, &Name, &UiDocumentOwner, &mut Node)>,
    mut foreground_visuals: Query<(&ChildOf, &mut ImageNode), With<UiVisualLayer>>,
) {
    let Some(species_view) = species_index.get(&species_assets) else {
        return;
    };
    for (row_entity, row, tooltip) in &rows {
        let Ok((list_policy, document_owner)) = lists.get(row.list) else {
            continue;
        };
        let Ok(information_panel) = information_panels.get(document_owner.0) else {
            continue;
        };
        let Ok(animal_needs) = animals.get(information_panel.subject) else {
            continue;
        };
        let Some((need_kind, authored_icon_index, wellness_q16)) =
            selected_animal_need_row_value(list_policy.source, row.index, animal_needs)
        else {
            continue;
        };
        let Some(species_need) = species_view
            .find(animal_needs.0.species)
            .and_then(|species| species.needs.iter().find(|need| need.kind == need_kind))
        else {
            continue;
        };

        // The icon names its need when hovered, as the original's tooltip.
        let tooltip_key = AssetId::from_key(need_tooltip_key(need_kind));
        if tooltip.map(|tooltip| tooltip.0) != Some(tooltip_key) {
            commands
                .entity(row_entity)
                .insert(UiShortTooltipKey(tooltip_key));
        }

        for (owner, mut icon_value) in &mut multi_icons {
            if owner.0 == row_entity && icon_value.0 != authored_icon_index {
                icon_value.0 = authored_icon_index;
            }
        }

        let wellness_permille =
            u16::try_from(wellness_q16.clamp(0, MAX_NEED_Q16) / Q16_ONE).unwrap_or_default();
        let foreground_height_percent = 100.0 - f32::from(wellness_permille) / 10.0;
        let foreground_color = if wellness_permille
            <= species_need.critical_wellness_threshold.unwrap_or_default()
        {
            Color::srgb(1.0, 0.0, 20.0 / 255.0)
        } else if wellness_permille <= species_need.pressing_wellness_threshold.unwrap_or_default()
        {
            Color::srgb(1.0, 1.0, 0.0)
        } else {
            Color::srgb(0.0, 1.0, 0.0)
        };

        for (foreground_entity, name, owner, mut node) in &mut foreground_layouts {
            if owner.0 != row_entity || !name.as_str().eq_ignore_ascii_case("icon foreground") {
                continue;
            }
            // The colour fills the icon from the bottom up as the need grows.
            let next_height = Val::Percent(foreground_height_percent);
            let next_top = Val::Percent(100.0 - foreground_height_percent);
            if node.height != next_height || node.top != next_top {
                node.height = next_height;
                node.top = next_top;
            }
            for (parent, mut image) in &mut foreground_visuals {
                if parent.parent() == foreground_entity && image.color != foreground_color {
                    image.color = foreground_color;
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

/// The `needs.xml` label naming an animal need, such as "Hunger:".
fn need_tooltip_key(need: NeedKind) -> &'static str {
    match need {
        NeedKind::Environment => "entityneeds:animal_environment",
        NeedKind::Hunger => "entityneeds:animal_hunger",
        NeedKind::Thirst => "entityneeds:animal_thirst",
        NeedKind::Rest => "entityneeds:animal_rest",
        NeedKind::Exercise => "entityneeds:animal_exercise",
        NeedKind::Privacy => "entityneeds:animal_privacy",
        NeedKind::Hygiene => "entityneeds:animal_hygiene",
        NeedKind::Social => "entityneeds:animal_social",
        NeedKind::Stimulation => "entityneeds:animal_stimulation",
        NeedKind::Reproduction => "entityneeds:animal_reproduction",
        NeedKind::Health | NeedKind::Bathroom => "entityneeds:animal_well",
    }
}

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

#[cfg(test)]
mod tooltip_tests {
    use super::*;

    #[test]
    fn each_shown_need_has_its_own_label() {
        let shown = [
            NeedKind::Environment,
            NeedKind::Hunger,
            NeedKind::Thirst,
            NeedKind::Rest,
            NeedKind::Exercise,
            NeedKind::Privacy,
            NeedKind::Hygiene,
            NeedKind::Social,
            NeedKind::Stimulation,
        ];
        let keys: std::collections::HashSet<_> = shown.into_iter().map(need_tooltip_key).collect();
        assert_eq!(keys.len(), shown.len());
        assert_eq!(
            need_tooltip_key(NeedKind::Hunger),
            "entityneeds:animal_hunger"
        );
    }
}
