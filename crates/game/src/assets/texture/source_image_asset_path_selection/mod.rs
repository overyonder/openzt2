//! Bevy asset-path selection for original Blue Fang image sources.

pub(crate) fn select_bevy_image_asset_path_for_blue_fang_source_image(source_path: &str) -> String {
    let source_path = source_path.replace('\\', "/").to_ascii_lowercase();
    match source_path.rsplit_once('.') {
        Some((stem, extension @ ("cur" | "dds"))) => format!("{stem}.z2{extension}"),
        _ => source_path,
    }
}
