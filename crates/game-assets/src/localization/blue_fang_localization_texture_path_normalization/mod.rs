pub(super) fn normalize_blue_fang_localization_texture_path(path: &str) -> String {
    let normalized_path = path.trim().replace('\\', "/").to_ascii_lowercase();
    normalized_path.rsplit_once('.').map_or_else(
        || format!("{normalized_path}.dds"),
        |(stem, _)| format!("{stem}.dds"),
    )
}
