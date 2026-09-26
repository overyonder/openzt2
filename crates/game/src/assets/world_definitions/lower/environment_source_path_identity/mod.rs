use super::world_definition_source_value_reading_and_conversion::id;
use openzt2_game_data::AssetId;

pub(super) fn environment_id_from_path(path: &str) -> AssetId {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map_or(path, |(stem, _)| stem);
    let family = stem
        .trim_end_matches("_fog")
        .trim_end_matches("lights")
        .trim_end_matches("Lights");
    id(if family.is_empty() { "default" } else { family })
}
pub(super) fn environment_family_from_path(path: &str) -> AssetId {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map_or(path, |(stem, _)| stem)
        .to_ascii_lowercase();
    let stem = stem
        .strip_suffix("_small")
        .or_else(|| stem.strip_suffix("_medium"))
        .unwrap_or(stem.as_str());
    let family = stem.strip_prefix("env1").unwrap_or(stem);
    id(if family.is_empty() || family == "noskirt" {
        "default"
    } else {
        family
    })
}
