//! Localization asset loading, precedence indexing, and borrowed runtime queries.

pub(crate) mod loaded_localization_queries;
pub(crate) mod localization_asset_loading;
pub(crate) mod localization_asset_types;
pub(crate) mod localization_precedence_index;

mod blue_fang_localization_catalog_lowering;
mod blue_fang_localization_date_format_tokenization;
mod blue_fang_localization_entry_lowering;
mod blue_fang_localization_format_tokenization;
mod blue_fang_localization_locale_recognition;
mod blue_fang_localization_presentation_lowering;
mod blue_fang_localization_texture_path_normalization;
mod fallback_localization_formatting;

use bevy::prelude::*;

use localization_asset_loading::register_localization_catalog_asset_and_loader;
use localization_precedence_index::{
    index_loaded_localization_entries_after_asset_events,
    load_english_localization_assets_after_archive_revision, LocalizationPrecedenceIndex,
};

pub(crate) struct LocalizationAssetPlugin;

impl Plugin for LocalizationAssetPlugin {
    fn build(&self, application: &mut App) {
        register_localization_catalog_asset_and_loader(application);
        application
            .init_resource::<LocalizationPrecedenceIndex>()
            .add_systems(
                PreUpdate,
                (
                    load_english_localization_assets_after_archive_revision,
                    index_loaded_localization_entries_after_asset_events,
                )
                    .chain(),
            );
    }
}
