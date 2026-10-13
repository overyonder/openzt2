use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::asset_source::AssetArchives;

pub(super) struct LiveD3d9EffectIncludeSourceCollector<'a> {
    asset_archives: &'a AssetArchives,
    include_source_paths: BTreeSet<PathBuf>,
}

impl<'a> LiveD3d9EffectIncludeSourceCollector<'a> {
    pub(super) fn new(asset_archives: &'a AssetArchives) -> Self {
        Self {
            asset_archives,
            include_source_paths: BTreeSet::new(),
        }
    }

    pub(super) fn finish(self) -> BTreeSet<PathBuf> {
        self.include_source_paths
    }
}

impl d3d9_effects::effect_types::D3d9EffectIncludeResolver
    for LiveD3d9EffectIncludeSourceCollector<'_>
{
    fn open(
        &mut self,
        parent_effect_path: &Path,
        requested_include_path: &Path,
    ) -> Result<(PathBuf, Vec<u8>), d3d9_effects::error::D3d9EffectProcessingError> {
        let resolved_include_path = self
            .asset_archives
            .resolve_effect_include(
                parent_effect_path,
                &requested_include_path.to_string_lossy(),
            )
            .ok_or_else(|| {
                d3d9_effects::error::D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                    parent_effect_path: parent_effect_path.to_owned(),
                    requested_include_path: requested_include_path.to_owned(),
                }
            })?;
        let include_source_bytes = self
            .asset_archives
            .read_source(&resolved_include_path)
            .map_err(|_| {
                d3d9_effects::error::D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                    parent_effect_path: parent_effect_path.to_owned(),
                    requested_include_path: requested_include_path.to_owned(),
                }
            })?;
        let repaired_include_source_bytes = decode_and_repair_d3d9_effect_source(
            &resolved_include_path,
            &include_source_bytes,
        )
        .map_err(|_| {
            d3d9_effects::error::D3d9EffectProcessingError::EffectIncludeCouldNotBeResolved {
                parent_effect_path: parent_effect_path.to_owned(),
                requested_include_path: requested_include_path.to_owned(),
            }
        })?;
        self.include_source_paths
            .insert(resolved_include_path.clone());
        Ok((resolved_include_path, repaired_include_source_bytes))
    }
}

pub(super) fn resolve_blue_fang_d3d9_effect_source_path(
    authored_effect_reference: &str,
) -> PathBuf {
    let normalized_effect_reference = authored_effect_reference
        .trim()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let authored_effect_path = Path::new(&normalized_effect_reference);
    let authored_effect_path = if authored_effect_path.extension().is_some() {
        authored_effect_path.to_owned()
    } else {
        authored_effect_path.with_extension("fx")
    };
    if authored_effect_path.components().count() == 1 {
        Path::new("effects").join(authored_effect_path)
    } else {
        authored_effect_path
    }
}

pub(super) fn read_and_repair_winning_d3d9_effect_source(
    asset_archives: &AssetArchives,
    effect_source_path: &Path,
) -> std::io::Result<Vec<u8>> {
    let effect_source_bytes = asset_archives.read_source(effect_source_path)?;
    decode_and_repair_d3d9_effect_source(effect_source_path, &effect_source_bytes)
}

fn decode_and_repair_d3d9_effect_source(
    effect_source_path: &Path,
    effect_source_bytes: &[u8],
) -> std::io::Result<Vec<u8>> {
    let (_, decoded_effect_source) =
        z2f::text::decoding::decode_blue_fang_source_text_from_utf8_or_utf16_bytes(
            effect_source_bytes,
        )?;
    Ok(
        z2f::text::d3d9_effect_syntax::repair_observed_blue_fang_d3d9_effect_source_syntax(
            effect_source_path,
            &decoded_effect_source,
        )
        .into_bytes(),
    )
}
