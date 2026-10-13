use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;
use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::selected_biome_ui_projection::UiBiomePanel;

pub(super) fn project_biome_panel_visibility(
    policy: Res<ConstructionPlacementPolicy>,
    mut panels: Query<(Ref<UiBiomePanel>, &mut Visibility)>,
) {
    for (panel, mut visibility) in &mut panels {
        if !policy.is_changed() && !panel.is_added() {
            continue;
        }
        let wanted = if panel.biome == policy.selected_biome {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

pub(super) fn project_selected_biome_name(
    policy: Res<ConstructionPlacementPolicy>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut fields: Query<(&UiTextBinding, &mut Text)>,
) {
    if !policy.is_changed()
        && !definitions.is_changed()
        && !active_localization.is_changed()
        && !localizations.is_changed()
    {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let value = definitions
        .biomes()
        .find(|biome| biome.id.0 == policy.selected_biome.0)
        .and_then(|biome| {
            localization.find_plain_localized_text(openzt2_game_data::AssetId(biome.name_key.0))
        })
        .unwrap_or("");
    for (binding, mut text) in &mut fields {
        if matches!(
            binding.0,
            UiTextPropertyBindingSource::SelectedTerrainBiomeName
        ) {
            text.0.clear();
            text.0.push_str(value);
        }
    }
}
