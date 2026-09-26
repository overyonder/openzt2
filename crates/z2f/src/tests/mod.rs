use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{ArchiveSet, ResolvedAssetChange, ResolvedAssetChangeKind};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "openzt2-z2f-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn archive(&self, name: &str, entries: &[(&str, &[u8])]) -> std::io::Result<PathBuf> {
        let path = self.0.join(name);
        let mut writer = ZipWriter::new(File::create(&path)?);
        let options = SimpleFileOptions::default();
        for (entry, bytes) in entries {
            writer
                .start_file(*entry, options)
                .map_err(std::io::Error::other)?;
            writer.write_all(bytes)?;
        }
        writer.finish().map_err(std::io::Error::other)?;
        Ok(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn later_archive_wins_then_disable_and_reenable_restore_the_overlay() -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let base = directory.archive(
        "x300_000.z2f",
        &[
            ("UI/Layout/Menu.XML", b"base"),
            ("base-only.txt", b"base-only"),
        ],
    )?;
    let first_mod = directory.archive(
        "first.z2f",
        &[
            ("ui/layout/menu.xml", b"first"),
            ("first-only.txt", b"first-only"),
        ],
    )?;
    let last_mod = directory.archive(
        "last.z2f",
        &[
            ("UI\\LAYOUT\\MENU.XML", b"last"),
            ("last-only.txt", b"last-only"),
        ],
    )?;
    let archives = ArchiveSet::open([base, first_mod, last_mod])?;

    assert_eq!(archives.read(Path::new("ui/layout/menu.xml"))?, b"last");
    assert_eq!(archives.read(Path::new("MENU.XML"))?, b"last");

    let disabled = archives.set_archive_enabled(2, false)?;
    assert_eq!(archives.read(Path::new("ui/layout/menu.xml"))?, b"first");
    assert_eq!(
        disabled,
        vec![
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("last-only.txt"),
                change_kind: ResolvedAssetChangeKind::Removed,
            },
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("menu.xml"),
                change_kind: ResolvedAssetChangeKind::Modified,
            },
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("ui/layout/menu.xml"),
                change_kind: ResolvedAssetChangeKind::Modified,
            },
        ]
    );

    let restored = archives.set_archive_enabled(2, true)?;
    assert_eq!(archives.read(Path::new("ui/layout/menu.xml"))?, b"last");
    assert_eq!(
        restored,
        vec![
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("last-only.txt"),
                change_kind: ResolvedAssetChangeKind::Added,
            },
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("menu.xml"),
                change_kind: ResolvedAssetChangeKind::Modified,
            },
            ResolvedAssetChange {
                normalized_asset_path: PathBuf::from("ui/layout/menu.xml"),
                change_kind: ResolvedAssetChangeKind::Modified,
            },
        ]
    );
    Ok(())
}

#[test]
fn unresolved_qualified_source_reference_uses_its_unique_basename() -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let base = directory.archive(
        "x300_000.z2f",
        &[("biomes/shared/groundbrush01_64.dds", b"brush")],
    )?;
    let archives = ArchiveSet::open([base])?;

    assert_eq!(
        archives.resolve_source_reference(
            Path::new("biomes/borealforest.xml"),
            "Biomes/grassland/groundbrush01_64.DDS",
        ),
        Some(PathBuf::from("biomes/shared/groundbrush01_64.dds")),
    );
    Ok(())
}

#[test]
fn extensionless_model_texture_reference_selects_an_image_instead_of_same_named_source_document(
) -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let base = directory.archive(
        "x300_000.z2f",
        &[
            ("entities/stations/ai/groundstation.xml", b"definition"),
            ("shared/GroundStation.dds", b"texture"),
        ],
    )?;
    let archives = ArchiveSet::open([base])?;

    assert_eq!(
        archives.resolve_model_texture_reference(
            Path::new("entities/stations/Materials/GroundStation.bfmat"),
            "groundstation",
        ),
        Some(PathBuf::from("shared/GroundStation.dds")),
    );
    Ok(())
}

#[test]
fn material_texture_resolution_preserves_local_detail_and_archive_precedence() -> std::io::Result<()>
{
    let directory = TestDirectory::new()?;
    let base = directory.archive(
        "x300_000.z2f",
        &[
            ("entities/lilies/lilies.dds", b"normal"),
            ("entities/lilies/low/lilies.dds", b"low"),
            ("elsewhere/lilies.tga", b"global alias"),
            ("ui/globe/post.dds", b"globe"),
            ("ui/fx/anchor/post.dds", b"anchor"),
            ("ui/globe/post.tga", b"legacy globe"),
        ],
    )?;
    let overlay = directory.archive(
        "override.z2f",
        &[("entities/lilies/lilies.dds", b"modded normal")],
    )?;
    let archives = ArchiveSet::open([base, overlay])?;
    for (from, reference, expected) in [
        (
            "entities/lilies/materials/leaves.bfmat",
            "lilies",
            "entities/lilies/lilies.dds",
        ),
        (
            "entities/lilies/low/materials/leaves.bfmat",
            "lilies",
            "entities/lilies/low/lilies.dds",
        ),
        ("ui/globe/materials/ring.bfmat", "post", "ui/globe/post.dds"),
        ("ui/globe/selected.nif", "post.tga", "ui/globe/post.tga"),
    ] {
        assert_eq!(
            archives.resolve_model_texture_reference(Path::new(from), reference),
            Some(PathBuf::from(expected))
        );
    }
    let path = Path::new("entities/lilies/lilies.dds");
    assert_eq!(archives.read(path)?, b"modded normal");
    archives.set_archive_enabled(1, false)?;
    assert_eq!(archives.read(path)?, b"normal");
    Ok(())
}

#[test]
fn blue_fang_renderer_model_prefers_bfb_and_retains_nif_fallback() -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let base = directory.archive(
        "x300_000.z2f",
        &[
            ("models/paired.nif", b"nif"),
            ("models/paired.bfb", b"bfb"),
            ("models/fallback.nif", b"nif"),
        ],
    )?;
    let archives = ArchiveSet::open([base])?;

    assert_eq!(
        archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new("entities/object.xml"),
            "models/paired.nif",
            true,
        ),
        Some(PathBuf::from("models/paired.bfb")),
    );
    assert_eq!(
        archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new("entities/object.xml"),
            "models/fallback.nif",
            true,
        ),
        Some(PathBuf::from("models/fallback.nif")),
    );
    assert_eq!(
        archives.resolve_model_reference(Path::new("entities/object.xml"), "models/paired.nif"),
        Some(PathBuf::from("models/paired.nif")),
    );

    assert_eq!(
        archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new("entities/object.xml"),
            "models/paired",
            true,
        ),
        Some(PathBuf::from("models/paired.bfb")),
    );
    assert_eq!(
        archives.resolve_model_reference_with_blue_fang_bfb_preference(
            Path::new("entities/object.xml"),
            "models/fallback",
            true,
        ),
        Some(PathBuf::from("models/fallback.nif")),
    );
    Ok(())
}
