use std::path::{Path, PathBuf};

use crate::{
    archive_overlay::winning_archive_entry_for_path,
    paths::normalize_asset_path_for_case_insensitive_archive_lookup, ArchiveSet,
};

impl ArchiveSet {
    /// Resolves an authored source reference without inflating any archive
    /// entry. Exact virtual paths, paths relative to the referring document,
    /// and unique source basenames are accepted.
    #[must_use]
    pub fn resolve_source_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        const SOURCE_EXTENSIONS: [&str; 16] = [
            "xml", "dl", "maxml", "zt2", "tsk", "beh", "trk", "old", "bfm", "dat", "dds", "jpg",
            "jpeg", "png", "bmp", "tga",
        ];

        self.resolve_reference(from, reference, &SOURCE_EXTENSIONS)
    }

    /// Resolves one D3D9 Effects include as an independent overlay entry.
    #[must_use]
    pub fn resolve_effect_include(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.resolve_reference(from, reference, &["fx", "fxh", "h"])
    }

    /// Resolves a Lua source or bytecode reference without inflating it.
    #[must_use]
    pub fn resolve_script_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        const EXTENSIONS: [&str; 3] = ["lua", "bin", "luac"];
        self.resolve_reference(from, reference, &EXTENSIONS)
            .or_else(|| {
                let path = Path::new(reference.trim());
                path.extension()
                    .and_then(|_| path.with_extension("").to_str().map(str::to_owned))
                    .and_then(|stem| self.resolve_reference(from, &stem, &EXTENSIONS))
            })
    }

    /// Resolves an authored audio-clip reference without inflating it.
    #[must_use]
    pub fn resolve_audio_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.resolve_reference(from, reference, &["wav", "mp3", "ogg"])
    }

    /// Resolves an authored native model reference without inflating it.
    #[must_use]
    pub fn resolve_model_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.resolve_model_reference_with_blue_fang_bfb_preference(from, reference, false)
    }

    /// Resolves an authored native model reference, honoring the Blue Fang
    /// renderer selector even when XML names a legacy NIF. Retains the
    /// authored NIF when no BFB with that stem is available.
    #[must_use]
    pub fn resolve_model_reference_with_blue_fang_bfb_preference(
        &self,
        from: &Path,
        reference: &str,
        prefer_blue_fang_bfb: bool,
    ) -> Option<PathBuf> {
        const NETIMMERSE_FIRST_EXTENSIONS: [&str; 2] = ["nif", "bfb"];
        const BLUE_FANG_BFB_FIRST_EXTENSIONS: [&str; 2] = ["bfb", "nif"];
        if prefer_blue_fang_bfb {
            let bfb_path = Path::new(reference.trim()).with_extension("bfb");
            if let Some(winner) = bfb_path.to_str().and_then(|reference| {
                self.resolve_reference(from, reference, &BLUE_FANG_BFB_FIRST_EXTENSIONS)
            }) {
                return Some(winner);
            }
        }
        let extensions = if prefer_blue_fang_bfb {
            &BLUE_FANG_BFB_FIRST_EXTENSIONS
        } else {
            &NETIMMERSE_FIRST_EXTENSIONS
        };
        self.resolve_reference(from, reference, extensions)
            .or_else(|| {
                let path = Path::new(reference.trim());
                path.extension()
                    .and_then(|_| path.with_extension("").to_str().map(str::to_owned))
                    .and_then(|stem| self.resolve_reference(from, &stem, extensions))
            })
    }

    /// Resolves model and material textures in their local directory tree
    /// before falling back to globally unique basenames. Materials commonly
    /// live beneath `Materials`, beside normal- and low-detail images.
    #[must_use]
    pub fn resolve_model_texture_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        const IMAGE_EXTENSIONS: [&str; 6] = ["dds", "tga", "jpg", "bmp", "png", "jpeg"];
        let normalized_reference =
            normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(reference.trim()));
        let name = normalized_reference.file_name()?;
        if normalized_reference.extension().is_some_and(|extension| {
            !IMAGE_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        }) {
            return None;
        }
        let parent = normalize_asset_path_for_case_insensitive_archive_lookup(from)
            .parent()?
            .to_owned();
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let local_directories = parent
            .ancestors()
            .flat_map(|ancestor| [ancestor.join(name), ancestor.join("shared").join(name)]);
        for candidate in [
            normalized_reference.clone(),
            parent.join(&normalized_reference),
        ]
        .into_iter()
        .chain(local_directories)
        {
            let candidates = std::iter::once(candidate.clone()).chain(
                IMAGE_EXTENSIONS
                    .iter()
                    .filter(|_| normalized_reference.extension().is_none())
                    .map(|extension| candidate.with_extension(extension)),
            );
            for candidate in candidates {
                // A root basename must not take the global alias shortcut
                // while more specific model directories remain to be tried.
                if let Some(winner) = state.winning_entries_by_normalized_path.get(&candidate) {
                    return Some(winner.original_archive_path.clone());
                }
            }
        }
        drop(state);
        self.resolve_reference(from, reference, &IMAGE_EXTENSIONS)
    }

    /// Resolves one UI-authored media or source reference against the current
    /// priority overlay. UI lowering asks for concrete references only; it
    /// never needs a cloned view of the complete archive overlay.
    pub fn resolve_ui_reference(&self, from: &Path, reference: &str) -> Option<PathBuf> {
        self.resolve_reference(
            from,
            reference,
            &[
                "bmp", "cur", "dds", "jpg", "jpeg", "png", "tga", "nif", "bfb", "wav", "mp3",
                "ogg", "xml", "zt2", "old",
            ],
        )
    }

    fn resolve_reference(
        &self,
        from: &Path,
        reference: &str,
        extensions: &[&str],
    ) -> Option<PathBuf> {
        let reference =
            normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(reference.trim()));
        if reference.as_os_str().is_empty() {
            return None;
        }
        if reference.extension().is_some_and(|extension| {
            !extensions
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        }) {
            return None;
        }
        let state = self
            .overlay_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let parent = normalize_asset_path_for_case_insensitive_archive_lookup(from)
            .parent()
            .map(Path::to_owned)
            .unwrap_or_default();
        let direct = [reference.clone(), parent.join(&reference)]
            .into_iter()
            .find_map(|candidate| {
                winning_archive_entry_for_path(&state, &candidate)
                    .map(|winner| winner.original_archive_path.clone())
            });
        if direct.is_some() {
            return direct;
        }
        if let Some(winner) = reference
            .file_name()
            .and_then(|name| winning_archive_entry_for_path(&state, Path::new(name)))
        {
            return Some(winner.original_archive_path.clone());
        }
        if reference.extension().is_none() {
            for extension in extensions {
                let relative = parent.join(&reference).with_extension(extension);
                if let Some(winner) = winning_archive_entry_for_path(&state, &relative) {
                    return Some(winner.original_archive_path.clone());
                }
                let basename = reference.with_extension(extension);
                if let Some(winner) = winning_archive_entry_for_path(&state, &basename) {
                    return Some(winner.original_archive_path.clone());
                }
            }
        }
        None
    }
}
