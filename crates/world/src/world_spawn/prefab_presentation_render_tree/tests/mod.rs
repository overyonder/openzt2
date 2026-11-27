use openzt2_game_data::scene_prefab::ScenePrefabDocument;

use super::*;
use crate::world_spawn::prefab_world_instance_spawning::{
    begin_rebuilding_world_instance_renderables_after_prefab_reload,
    PrefabInstanceRenderablesRebuilding,
};
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;

fn model_prefab(digest: [u8; 32]) -> ScenePrefabAsset {
    ScenePrefabAsset::from_test_document(ScenePrefabDocument {
        entities: Vec::new(),
        dependencies: Vec::new(),
        automatic_placement_bounds_xz: None,
        rail_camera: None,
    })
    .with_model_source_digest(digest)
}

#[test]
fn republishing_the_same_native_model_keeps_its_renderables() {
    let mut app = App::new();
    app.init_resource::<Assets<ScenePrefabAsset>>()
        .add_message::<AssetEvent<ScenePrefabAsset>>()
        .add_message::<ReplacedPrefabReloaded>()
        .add_systems(Update, publish_replaced_prefab_reloads)
        .add_systems(
            Update,
            (
                begin_rebuilding_world_instance_renderables_after_prefab_reload,
                rebuild_prefab_presentations_after_prefab_reload,
            )
                .after(publish_replaced_prefab_reloads),
        );
    let original = [1; 32];
    let world = app.world_mut();
    let prefab = world
        .resource_mut::<Assets<ScenePrefabAsset>>()
        .add(model_prefab(original));
    let instance = world
        .spawn((
            PrefabSourceAssetHandle(prefab.clone()),
            PrefabRenderableSource(Some(original)),
        ))
        .id();
    let presentation = world
        .spawn((
            PrefabPresentation(prefab.clone()),
            PrefabPresentationHydrated,
        ))
        .id();
    let render_root = world
        .spawn((
            PrefabPresentationRenderOf(presentation),
            PrefabRenderableSource(Some(original)),
        ))
        .id();

    let mut publish_model = |digest| {
        let world = app.world_mut();
        world
            .resource_mut::<Assets<ScenePrefabAsset>>()
            .insert(prefab.id(), model_prefab(digest))
            .unwrap();
        world.write_message(AssetEvent::Modified { id: prefab.id() });
        app.update();
        (
            app.world()
                .get::<PrefabInstanceRenderablesRebuilding>(instance)
                .is_some(),
            app.world().get_entity(render_root).is_ok(),
        )
    };

    assert_eq!(publish_model(original), (false, true));
    assert_eq!(publish_model([2; 32]), (true, false));
}
