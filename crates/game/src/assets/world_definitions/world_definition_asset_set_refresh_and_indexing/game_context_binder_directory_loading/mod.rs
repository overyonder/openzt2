use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

use crate::{
    asset_source::AssetArchives,
    assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

pub(super) fn load_game_context_binder_directories(
    archives: &AssetArchives,
) -> io::Result<BTreeSet<PathBuf>> {
    let path = "config/gamectxt.xml";
    let bytes = archives.read_source(Path::new(path))?;
    let document = parse_blue_fang_source_document(AssetPath::new(path), &bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut directories = BTreeSet::new();
    for binder_root in document
        .root
        .element_children()
        .filter(|node| node.name.eq_ignore_ascii_case("BFBinderRoot"))
    {
        for child in binder_root.element_children() {
            let Some(path) = child.attribute_named_any(&["path"]) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "game context binder directory has no path",
                ));
            };
            directories.insert(PathBuf::from(AssetPath::new(path).key()));
        }
    }
    Ok(directories)
}
