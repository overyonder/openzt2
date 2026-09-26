use std::{borrow::Borrow, fmt, ops::Deref, path::Path};

/// A canonical path inside an original content archive.
///
/// ZT2 archives use case-insensitive, slash-separated paths. Construction
/// normalizes Windows separators, repeated separators, leading separators and
/// `.` components, while retaining the original component spelling for viewer
/// presentation. Lookup code should use [`AssetPath::key`] when it needs the
/// archive's case-insensitive identity.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct AssetPath(String);

impl AssetPath {
    pub(crate) fn new(path: impl AsRef<str>) -> Self {
        Self(canonicalize(path.as_ref()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the case-insensitive identity used by Z2F archive lookup.
    pub(crate) fn key(&self) -> String {
        z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(Path::new(&self.0))
            .to_string_lossy()
            .into_owned()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for AssetPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for AssetPath {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for AssetPath {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Debug for AssetPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("AssetPath").field(&self.0).finish()
    }
}

impl fmt::Display for AssetPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for AssetPath {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl From<String> for AssetPath {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

fn canonicalize(path: &str) -> String {
    let mut canonical = String::with_capacity(path.len());
    for component in path.split(['/', '\\']) {
        match component {
            "" | "." => continue,
            ".." => {
                if let Some(separator) = canonical.rfind('/') {
                    canonical.truncate(separator);
                } else {
                    canonical.clear();
                }
            }
            component => {
                if !canonical.is_empty() {
                    canonical.push('/');
                }
                canonical.push_str(component);
            }
        }
    }
    canonical
}

#[cfg(test)]
mod tests {
    use super::AssetPath;

    #[test]
    fn canonicalizes_archive_paths_without_losing_display_case() {
        let path = AssetPath::new(r"/entities\\animals/./lion/../Tiger.XML");

        assert_eq!(path.as_str(), "entities/animals/Tiger.XML");
        assert_eq!(path.key(), "entities/animals/tiger.xml");
    }
}
