use crate::assets::source_document::path::AssetPath;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::AssetId;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

pub(super) fn dependency(value: Option<String>, output: &mut BuildOutput) -> AssetId {
    value
        .filter(|v| !v.is_empty())
        .map(|value| {
            let id = AssetId::from_virtual_path(&value);
            output.dependencies.insert(id.0);
            id
        })
        .unwrap_or_default()
}

/// UI image references point directly at the winning standard image path.
pub(super) fn texture_dependency(
    value: Option<String>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<AssetId> {
    value
        .filter(|value| !value.is_empty())
        .map(|path| {
            resolved_dependency_path(&path, &input.resolved_dependencies.images, "image", input)
                .map(|winner| dependency(Some(winner), output))
        })
        .transpose()
        .map(|asset| asset.unwrap_or_default())
}

pub(super) fn cursor_texture_dependency(
    value: Option<String>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<AssetId> {
    let id = match texture_dependency(value.clone(), output, input) {
        Ok(id) => id,
        Err(error) => {
            bevy::log::warn!(
                document = input.source.path.as_str(),
                cursor = ?value,
                %error,
                "Cursor is unavailable; using the default cursor"
            );
            return Ok(AssetId::default());
        }
    };
    if id != AssetId::default() {
        output.interactive_textures.insert(id.0);
    }
    Ok(id)
}

/// Static images recorded as absent render transparently.
pub(super) fn visual_texture_dependency(
    value: Option<String>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<AssetId> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(AssetId::default());
    };
    let key = AssetPath::new(&value).key();
    if input
        .resolved_dependencies
        .absent_visual_images
        .contains(&key)
    {
        return Ok(AssetId::default());
    }
    texture_dependency(Some(value), output, input)
}

pub(super) fn resolved_dependency_path(
    authored: &str,
    winners: &BTreeMap<String, String>,
    kind: &str,
    input: &AuthoredUiDocument,
) -> io::Result<String> {
    let key = AssetPath::new(authored).key();
    winners
        .get(&format!("{}\0{key}", input.source.path.key()))
        .or_else(|| winners.get(&format!("*\0{key}")))
        .cloned()
        .or_else(|| {
            winners
                .values()
                .any(|winner| winner == &key)
                .then_some(key.clone())
        })
        .or_else(|| {
            key.rsplit('/')
                .next()
                .and_then(|basename| winners.get(&format!("@basename\0{basename}")))
                .cloned()
        })
        .or_else(|| {
            let basename = key.rsplit('/').next()?;
            let matches = winners
                .values()
                .filter(|winner| winner.rsplit('/').next() == Some(basename))
                .collect::<BTreeSet<_>>();
            (matches.len() == 1)
                .then(|| matches.into_iter().next().cloned())
                .flatten()
        })
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("UI {kind} reference {authored:?} has no planner-selected winner"),
            )
        })
}

pub(super) fn optional_resolved_audio_dependency_path(
    authored: &str,
    input: &AuthoredUiDocument,
) -> io::Result<Option<String>> {
    match resolved_dependency_path(authored, &input.resolved_dependencies.audio, "audio", input) {
        Ok(path) => Ok(Some(path)),
        Err(_) if std::path::Path::new(authored).extension().is_none() => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn resolved_asset_dependency(
    value: Option<String>,
    winners: &BTreeMap<String, String>,
    kind: &str,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<AssetId> {
    value
        .map(|path| {
            resolved_dependency_path(&path, winners, kind, input)
                .map(|winner| dependency(Some(winner), output))
        })
        .transpose()
        .map(|dependency| dependency.unwrap_or_default())
}
