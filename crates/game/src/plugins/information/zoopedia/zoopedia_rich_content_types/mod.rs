use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{localization::LocalizationRichContentNode, AssetId};

use crate::assets::localization::loaded_localization_queries::LoadedLocalizationView;
use crate::plugins::ui::{
    authored_ui_node_projection_components::UiDocumentOwner,
    authored_ui_text_content_binding::UiTextBinding,
    authored_ui_text_layout_presentation::UiPhysicalFontSize,
};

/// Text nodes and backgrounds used to display a Zoopedia page.
#[derive(SystemParam)]
pub(in crate::plugins::information) struct ZoopediaPageTextNodeProjectionQueries<'w, 's> {
    pub(super) added_text_targets:
        Query<'w, 's, (&'static UiDocumentOwner, Ref<'static, UiTextBinding>)>,
    pub(super) text_nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static UiDocumentOwner,
            &'static UiTextBinding,
            &'static mut Node,
            &'static ChildOf,
            Option<&'static mut Text>,
            &'static mut TextFont,
            &'static UiPhysicalFontSize,
            &'static mut TextColor,
            &'static mut UiTransform,
        ),
        Without<ZoopediaLocalizedTextBackground>,
    >,
    pub(super) localized_text_backgrounds: Query<
        'w,
        's,
        (
            Entity,
            &'static ZoopediaLocalizedTextBackground,
            &'static mut ImageNode,
            &'static mut Node,
        ),
        Without<UiTextBinding>,
    >,
    pub(super) localized_rich_content_roots:
        Query<'w, 's, (Entity, &'static ZoopediaLocalizedRichContentRoot)>,
}

/// A Zoopedia page and its target UI document.
pub(super) struct LocalizedZoopediaPageTextProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) zoopedia_subject: AssetId,
    pub(super) zoopedia_page_entity: Entity,
    pub(super) localized_title_key: AssetId,
    pub(super) localized_title_text: Option<&'a str>,
    pub(super) localized_body_key: AssetId,
    pub(super) localized_plain_body_text: Option<&'a str>,
}

/// Localized title and its text node.
pub(super) struct LocalizedZoopediaTitleTextNodeProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) localized_title_key: Option<AssetId>,
    pub(super) localized_title_text: Option<&'a str>,
    pub(super) authored_text_entity: Entity,
    pub(super) existing_text: Option<&'a mut Text>,
    pub(super) authored_text_font: &'a mut TextFont,
    pub(super) authored_text_color: &'a mut TextColor,
    pub(super) authored_text_transform: &'a mut UiTransform,
}

/// Localized body text and the nodes replaced when it changes.
pub(super) struct LocalizedZoopediaBodyTextNodeProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) localized_body_key: AssetId,
    pub(super) localized_plain_body_text: Option<&'a str>,
    pub(super) zoopedia_page_entity: Entity,
    pub(super) authored_text_entity: Entity,
    pub(super) authored_parent_entity: Entity,
    pub(super) authored_node: &'a mut Node,
    pub(super) existing_text: Option<&'a mut Text>,
    pub(super) authored_text_font: &'a mut TextFont,
    pub(super) base_physical_font_size_pixels: f32,
    pub(super) authored_text_color: &'a mut TextColor,
    pub(super) authored_text_transform: &'a mut UiTransform,
}

/// Localized rich text and its base formatting.
pub(super) struct LocalizedZoopediaRichContentTreeProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) localized_content_key: AssetId,
    pub(super) zoopedia_page_entity: Entity,
    pub(super) authored_text_entity: Entity,
    pub(super) base_text_font: &'a TextFont,
    pub(super) base_physical_font_size_pixels: f32,
    pub(super) base_text_color: &'a TextColor,
}

/// Content and layout for one rich-text record.
pub(super) struct LocalizedZoopediaRichContentRecordProjectionInput<'a> {
    pub(super) asset_server: &'a AssetServer,
    pub(super) source_record: &'a LocalizationRichContentNode,
    pub(super) source_records: &'a [LocalizationRichContentNode],
    pub(super) zoopedia_page_entity: Entity,
    pub(super) parent_entity: Entity,
    pub(super) width: Val,
    pub(super) height: Val,
    pub(super) flex_shrink: f32,
    pub(super) base_text_font: &'a TextFont,
    pub(super) base_physical_font_size_pixels: f32,
    pub(super) base_text_color: &'a TextColor,
    pub(super) has_preceding_sibling: bool,
    pub(super) has_block_content: bool,
    pub(super) has_link_content: bool,
    pub(super) is_text_span: bool,
}

/// Text node and layout used to position its background image.
pub(super) struct LocalizedZoopediaTextBackgroundProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) localized_content_key: AssetId,
    pub(super) authored_text_entity: Entity,
    pub(super) authored_parent_entity: Entity,
    pub(super) authored_text_node: &'a Node,
}

/// Text presentation and its localized source.
pub(super) struct LocalizedZoopediaTextPresentationProjectionInput<'a> {
    pub(super) localization: LoadedLocalizationView<'a>,
    pub(super) localized_content_key: AssetId,
    pub(super) authored_text_font: &'a mut TextFont,
    pub(super) authored_text_color: &'a mut TextColor,
    pub(super) authored_transform: &'a mut UiTransform,
    pub(super) apply_content_offset: bool,
}

/// Render-only image quad behind one localized Zoopedia text node.
#[derive(Component)]
pub(super) struct ZoopediaLocalizedTextBackground {
    pub(super) text: Entity,
}

/// Root of one independently projected localized Zoopedia rich-text layout.
#[derive(Component)]
pub(super) struct ZoopediaLocalizedRichContentRoot {
    pub(super) text: Entity,
}

/// Stable subject link for one interactive link inside projected Zoopedia text.
#[derive(Component, Debug, Clone, Copy)]
pub(in crate::plugins::information) struct ZoopediaRichLink {
    pub(in crate::plugins::information) page: Entity,
    pub(in crate::plugins::information) subject: AssetId,
}
