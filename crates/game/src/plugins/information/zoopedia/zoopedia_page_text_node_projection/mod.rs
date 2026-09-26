use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use crate::plugins::information::zoopedia::zoopedia_body_text_node_projection::project_localized_zoopedia_body_to_authored_text_node;
use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaBodyTextNodeProjectionInput, LocalizedZoopediaPageTextProjectionInput,
    LocalizedZoopediaTitleTextNodeProjectionInput, ZoopediaPageTextNodeProjectionQueries,
};
use crate::plugins::information::zoopedia::zoopedia_title_text_node_projection::project_localized_zoopedia_title_to_authored_text_node;

pub(super) fn project_resolved_zoopedia_page_text_to_authored_ui_nodes(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projection_queries: &mut ZoopediaPageTextNodeProjectionQueries<'_, '_>,
    input: LocalizedZoopediaPageTextProjectionInput<'_>,
) {
    let mut projected_title_target_count = 0_u32;
    let mut projected_body_target_count = 0_u32;
    for (
        text_entity,
        document_owner,
        text_binding,
        mut authored_node,
        authored_parent,
        mut existing_text,
        mut authored_font,
        physical_font_size,
        mut authored_color,
        mut authored_transform,
    ) in &mut projection_queries.text_nodes
    {
        if document_owner.0 != input.zoopedia_page_entity {
            continue;
        }
        match &text_binding.0 {
            UiTextPropertyBindingSource::ZoopediaTitle => {
                projected_title_target_count += 1;
                project_localized_zoopedia_title_to_authored_text_node(
                    commands,
                    LocalizedZoopediaTitleTextNodeProjectionInput {
                        localization: input.localization,
                        localized_title_key: Some(input.localized_title_key),
                        localized_title_text: input.localized_title_text,
                        authored_text_entity: text_entity,
                        existing_text: existing_text.as_deref_mut(),
                        authored_text_font: &mut authored_font,
                        authored_text_color: &mut authored_color,
                        authored_text_transform: &mut authored_transform,
                    },
                );
            }
            UiTextPropertyBindingSource::ZoopediaBody => {
                projected_body_target_count += 1;
                project_localized_zoopedia_body_to_authored_text_node(
                    commands,
                    asset_server,
                    &mut projection_queries.localized_text_backgrounds,
                    &projection_queries.localized_rich_content_roots,
                    LocalizedZoopediaBodyTextNodeProjectionInput {
                        localization: input.localization,
                        localized_body_key: input.localized_body_key,
                        localized_plain_body_text: input.localized_plain_body_text,
                        zoopedia_page_entity: input.zoopedia_page_entity,
                        authored_text_entity: text_entity,
                        authored_parent_entity: authored_parent.parent(),
                        authored_node: &mut authored_node,
                        existing_text: existing_text.as_deref_mut(),
                        authored_text_font: &mut authored_font,
                        base_physical_font_size_pixels: physical_font_size.physical_pixels(),
                        authored_text_color: &mut authored_color,
                        authored_text_transform: &mut authored_transform,
                    },
                );
            }
            _ => {}
        }
    }
    debug!(
        subject = ?input.zoopedia_subject,
        title_found = input.localized_title_text.is_some(),
        body_found = input.localized_plain_body_text.is_some(),
        projected_title_target_count,
        projected_body_target_count,
        "projected Zoopedia page"
    );
}
