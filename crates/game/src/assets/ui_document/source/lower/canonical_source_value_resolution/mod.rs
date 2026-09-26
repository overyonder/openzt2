//! Lowering of authored source values to canonical UI identifiers and help records.

use crate::assets::source_document::ui::model::SourceUiHelp;
use openzt2_game_data::ui_document::node::UiNodeHelpDefinition;
use openzt2_game_data::AssetId;

pub(super) fn lower_optional_authored_semantic_key_to_asset_id(value: Option<&str>) -> AssetId {
    value
        .filter(|value| !value.is_empty())
        .map(AssetId::from_key)
        .unwrap_or_default()
}

pub(super) fn lower_authored_encyclopedia_entry_reference_to_asset_id(
    value: Option<&str>,
) -> AssetId {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return AssetId::default();
    };
    let mut fields = value.split(':');
    match (fields.next(), fields.next(), fields.next(), fields.next()) {
        (Some(namespace), Some(subject), Some("entry"), None)
            if namespace.eq_ignore_ascii_case("zoopedia") =>
        {
            AssetId::from_key(&subject.to_ascii_lowercase())
        }
        _ => AssetId::from_key(value),
    }
}

pub(super) fn lower_authored_catalogue_key_to_asset_id(value: &str) -> AssetId {
    AssetId::from_key(&value.trim().replace('\\', "/").to_ascii_lowercase())
}

pub(super) fn lower_authored_ui_help_to_canonical_definition(
    value: Option<&SourceUiHelp>,
) -> UiNodeHelpDefinition {
    value
        .map(|value| {
            let derived = value
                .ids
                .as_deref()
                .filter(|ids| !ids.is_empty())
                .map(|ids| {
                    let mut key = String::with_capacity(ids.len() + "_lower".len());
                    key.push_str(ids);
                    let prefix_len = key.len();
                    ["_name", "_stt", "_ltt", "_help", "_lower"].map(|suffix| {
                        key.truncate(prefix_len);
                        key.push_str(suffix);
                        AssetId::from_key(&key)
                    })
                });
            let [name, short, long, help, lower] = derived.unwrap_or_else(|| {
                [
                    lower_optional_authored_semantic_key_to_asset_id(value.name.as_deref()),
                    lower_optional_authored_semantic_key_to_asset_id(value.short.as_deref()),
                    lower_optional_authored_semantic_key_to_asset_id(value.long.as_deref()),
                    lower_optional_authored_semantic_key_to_asset_id(value.help.as_deref()),
                    lower_optional_authored_semantic_key_to_asset_id(value.lower.as_deref()),
                ]
            });
            UiNodeHelpDefinition {
                name,
                short,
                long,
                help,
                lower,
            }
        })
        .unwrap_or_default()
}
