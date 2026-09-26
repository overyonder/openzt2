pub(super) fn normalize_scene_path(value: &str) -> String {
    let normalized = value.trim().replace('\\', "/").to_ascii_lowercase();
    crate::assets::model_source::native_model_source_lowering::
        native_model_scene_labelled_asset_path(&normalized)
}
