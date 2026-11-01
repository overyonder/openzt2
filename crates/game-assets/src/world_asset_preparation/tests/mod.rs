use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use bevy::{
    app::TaskPoolPlugin,
    asset::{io::AssetSourceId, AssetPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
use zip::{write::SimpleFileOptions, ZipWriter};

use super::WorldAssetPreparation;
use crate::{
    application_lifecycle::GamePhase,
    asset_source::create_bevy_asset_source_for_enabled_z2f_archives,
    species::{
        species_asset_types::{SpeciesAsset, SpeciesAssets},
        SpeciesAssetPlugin,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        WorldDefinitionAssetPlugin,
    },
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct MetadataArchiveDirectory(PathBuf);

impl MetadataArchiveDirectory {
    fn new() -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "openzt2-world-metadata-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn archive(&self) -> io::Result<PathBuf> {
        let path = self.0.join("metadata.z2f");
        let mut archive = ZipWriter::new(File::create_new(&path)?);
        for (name, source) in [
            (
                "config/timekeeper.xml",
                r#"<BFTimeKeeper timeScale="3600"><BFTIME_HEARTBEAT interval="0.02"/><BFTIME_SIMTICK interval="0.01"/><BFTIME_SLOWTICK interval="0.1"/></BFTimeKeeper>"#,
            ),
            (
                "entities/units/animals/ai/metadata.xml",
                "<BFAIEntityType/>",
            ),
        ] {
            archive
                .start_file(name, SimpleFileOptions::default())
                .map_err(io::Error::other)?;
            archive.write_all(source.as_bytes())?;
        }
        archive.finish().map_err(io::Error::other)?;
        Ok(path)
    }
}

impl Drop for MetadataArchiveDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn operation_demand_prepares_cold_world_metadata_without_leaving_main_menu() -> io::Result<()> {
    let directory = MetadataArchiveDirectory::new()?;
    let (source, archives) =
        create_bevy_asset_source_for_enabled_z2f_archives(vec![directory.archive()?])?;
    let mut app = App::new();
    app.register_asset_source(AssetSourceId::Default, source)
        .insert_resource(archives)
        .add_plugins((
            TaskPoolPlugin::default(),
            StatesPlugin,
            AssetPlugin::default(),
        ))
        .insert_state(GamePhase::MainMenu)
        .add_plugins((WorldDefinitionAssetPlugin, SpeciesAssetPlugin));

    app.update();
    assert!(app
        .world()
        .resource::<Assets<WorldDefinitionAsset>>()
        .is_empty());
    assert!(app.world().resource::<Assets<SpeciesAsset>>().is_empty());
    let operation = app.world_mut().spawn(WorldAssetPreparation).id();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update();
        let world = app.world();
        let definitions_ready = world
            .resource::<WorldDefinitions>()
            .get(world.resource::<Assets<WorldDefinitionAsset>>())
            .is_some();
        let species_ready = world
            .resource::<SpeciesAssets>()
            .get(world.resource::<Assets<SpeciesAsset>>())
            .is_some();
        if definitions_ready && species_ready {
            break;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other(format!(
                "metadata preparation did not finish: definitions={definitions_ready}, species={species_ready}"
            )));
        }
        std::thread::yield_now();
    }
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::MainMenu
    );
    assert!(app.world_mut().despawn(operation));
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::MainMenu
    );
    Ok(())
}
