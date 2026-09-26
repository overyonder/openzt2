use bevy::prelude::*;
use openzt2_game_data::{ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use crate::plugins::information::zoopedia::zoopedia_page_text_node_projection::project_resolved_zoopedia_page_text_to_authored_ui_nodes;
use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaPageTextProjectionInput, ZoopediaPageTextNodeProjectionQueries,
};
use crate::plugins::information::{
    zoopedia::zoopedia_hierarchy_operations::resolve_zoopedia_entry_subject,
    zoopedia::zoopedia_navigation_types::ZoopediaPage,
};

pub(in crate::plugins::information) fn project_localized_zoopedia_page_to_authored_information_document(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization_assets: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    zoopedia_pages: Query<(Entity, Ref<ZoopediaPage>)>,
    mut projection_queries: ZoopediaPageTextNodeProjectionQueries,
) {
    let source_assets_changed = world_definition_assets.is_changed()
        || localization_assets.is_changed()
        || active_localization_assets.is_changed();
    let Some(active_world_definitions) = active_world_definitions.get(&world_definition_assets)
    else {
        return;
    };
    let Some(localization) =
        active_localization_assets.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    for (zoopedia_page_entity, zoopedia_page) in &zoopedia_pages {
        let authored_text_target_added =
            projection_queries
                .added_text_targets
                .iter()
                .any(|(document_owner, text_binding)| {
                    document_owner.0 == zoopedia_page_entity
                        && text_binding.is_added()
                        && matches!(
                            &text_binding.0,
                            UiTextPropertyBindingSource::ZoopediaTitle
                                | UiTextPropertyBindingSource::ZoopediaBody
                        )
                });
        if !source_assets_changed && !zoopedia_page.is_changed() && !authored_text_target_added {
            continue;
        }
        let Some(zoopedia_entry) = active_world_definitions
            .zoopedia()
            .find(|entry| resolve_zoopedia_entry_subject(entry) == zoopedia_page.subject)
        else {
            continue;
        };
        let localized_title_key = AssetId(zoopedia_entry.title_key.0);
        let localized_body_key = AssetId(zoopedia_entry.body_key.0);
        project_resolved_zoopedia_page_text_to_authored_ui_nodes(
            &mut commands,
            &asset_server,
            &mut projection_queries,
            LocalizedZoopediaPageTextProjectionInput {
                localization,
                zoopedia_subject: zoopedia_page.subject,
                zoopedia_page_entity,
                localized_title_key,
                localized_title_text: localization.find_plain_localized_text(localized_title_key),
                localized_body_key,
                localized_plain_body_text: localization
                    .find_plain_localized_text(localized_body_key),
            },
        );
    }
}
