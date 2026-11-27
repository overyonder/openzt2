use bevy::{ecs::schedule::Schedules, window::WindowPlugin};

use super::*;

#[test]
fn full_game_schedules_initialize_headless() {
    let mut app = headless_game_app();
    for sub_app in app.sub_apps_mut().iter_mut() {
        let world = sub_app.world_mut();
        world.flush();
        let schedules: Vec<_> = world
            .resource::<Schedules>()
            .iter()
            .map(|(_, schedule)| schedule.label())
            .collect();
        for label in schedules {
            world.schedule_scope(label, |world, schedule| {
                schedule.initialize(world).unwrap_or_else(|error| {
                    panic!(
                        "could not initialize {label:?}: {}",
                        error.to_string(schedule.graph(), world)
                    )
                });
            });
        }
    }
}

#[test]
fn shared_asset_extensions_do_not_select_specialized_loaders() {
    use std::any::TypeId;

    use openzt2_game_assets::{
        texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    };

    let app = headless_game_app();
    let server = app.world().resource::<AssetServer>();
    bevy::tasks::block_on(async {
        for extension in ["xml", "zt2", "old", "dat", "dl", "bin", "h", "z2entry"] {
            let path = format!("untyped.{extension}");
            assert!(
                server.get_path_asset_loader(path).await.is_err(),
                "{extension} was claimed by a specialized loader"
            );
        }
        for extension in ["bmp", "dds", "jpg", "jpeg", "png", "tga"] {
            let loader = server
                .get_path_asset_loader(format!("texture.{extension}"))
                .await
                .unwrap();
            assert_eq!(loader.asset_type_id(), TypeId::of::<Image>(), "{extension}");
        }
        assert!(server.get_path_asset_loader("cursor.cur").await.is_err());

        let loader = server
            .get_path_asset_loader("ui/layout/mainmenu.xml.z2ui#Role/mainmenu")
            .await
            .unwrap();
        assert_eq!(loader.asset_type_id(), TypeId::of::<UiDocumentAsset>());

        let loader = server
            .get_path_asset_loader("cursor.cur.z2interactive#interactive-image")
            .await
            .unwrap();
        assert_eq!(
            loader.asset_type_id(),
            TypeId::of::<InteractiveTextureMetadataAsset>()
        );
    });
}

fn headless_game_app() -> App {
    let mut app = App::new();
    let (source, archives) =
        openzt2_game_assets::asset_source::create_empty_archive_asset_source_for_tests();
    app.register_asset_source(AssetSourceId::Default, source)
        .insert_resource(archives);
    add_original_save_asset_source(&mut app);
    app.add_plugins(
        game_platform_plugins()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::audio::AudioPlugin>()
            .disable::<bevy::gilrs::GilrsPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::app::PanicHandlerPlugin>()
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            }),
    );
    add_openzt2_game_plugins(&mut app);
    app.finish();
    app.cleanup();

    app
}
