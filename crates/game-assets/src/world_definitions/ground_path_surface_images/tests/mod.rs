use std::time::{Duration, Instant};

use bevy::{
    app::TaskPoolPlugin,
    asset::{io::AssetSourceId, AssetPlugin},
};

use crate::asset_source::{
    create_bevy_asset_source_for_enabled_z2f_archives, tests::TestDirectory,
};

use super::*;

#[derive(Default, TypePath)]
struct TestGroundPathImageLoader;

impl AssetLoader for TestGroundPathImageLoader {
    type Asset = Image;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        (): &(),
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(Image {
            data: Some(bytes),
            ..default()
        })
    }

    fn extensions(&self) -> &[&str] {
        &["z2dds"]
    }
}

#[test]
fn archive_image_override_reloads_the_ground_path_parent_and_its_image() -> std::io::Result<()> {
    let directory = TestDirectory::new()?;
    let base = directory.archive("x300_000.z2f", &[("path.dds", b"base")])?;
    let modification = directory.archive("mod.z2f", &[("path.dds", b"mod")])?;
    let (source, mut archives) =
        create_bevy_asset_source_for_enabled_z2f_archives(vec![base, modification])?;
    let mut app = App::new();
    app.register_asset_source(AssetSourceId::Default, source)
        .add_plugins((
            TaskPoolPlugin::default(),
            AssetPlugin {
                watch_for_changes_override: Some(true),
                use_asset_processor_override: Some(false),
                ..default()
            },
        ))
        .init_asset::<Image>()
        .init_asset::<GroundPathSurfaceImage>()
        .init_asset_loader::<TestGroundPathImageLoader>()
        .init_asset_loader::<GroundPathSurfaceImageLoader>();
    let handle = app
        .world()
        .resource::<AssetServer>()
        .load::<GroundPathSurfaceImage>(format!(
            "path.z2dds.{GROUND_PATH_SURFACE_IMAGE_EXTENSION}"
        ));

    let mut loaded = app
        .world()
        .resource::<Messages<AssetEvent<GroundPathSurfaceImage>>>()
        .get_cursor();
    for (enabled, expected) in [(true, b"mod".as_slice()), (false, b"base".as_slice())] {
        if !enabled {
            let _changed_paths = archives.set_archive_enabled(1, enabled)?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update();
            let world = app.world();
            let events = world.resource::<Messages<AssetEvent<GroundPathSurfaceImage>>>();
            if loaded
                .read(events)
                .any(|event| event.is_loaded_with_dependencies(handle.id()))
            {
                assert!(world
                    .resource::<AssetServer>()
                    .is_loaded_with_dependencies(handle.id()));
                let parent = world
                    .resource::<Assets<GroundPathSurfaceImage>>()
                    .get(&handle)
                    .unwrap();
                let image = world
                    .resource::<Assets<Image>>()
                    .get(&parent.image)
                    .unwrap();
                assert_eq!(image.data.as_deref(), Some(expected));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ground path image did not finish loading"
            );
            std::thread::yield_now();
        }
    }
    Ok(())
}
