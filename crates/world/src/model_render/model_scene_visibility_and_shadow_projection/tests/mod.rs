use bevy::asset::AssetPlugin;
use bevy::camera::visibility::VisibilitySystems;
use bevy::transform::components::TransformTreeChanged;
use bevy::world_serialization::WorldAsset;
use bevy::world_serialization::WorldSerializationPlugin;

use crate::model_render::model_scene_visibility_and_shadow_projection::*;

#[test]
fn rebuilt_world_instance_receives_model_policy_before_visibility_collection() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        WorldSerializationPlugin,
    ))
    .register_type::<Mesh3d>()
    .register_type::<Transform>()
    .register_type::<GlobalTransform>()
    .register_type::<TransformTreeChanged>()
    .add_observer(invalidate_model_scene_visibility_and_shadow_policy_on_world_instance_ready)
    .add_systems(
        PostUpdate,
        (
            project_model_scene_visibility_and_shadow_policy
                .before(VisibilitySystems::CheckVisibility),
            assert_portrait_mesh_policy_at_visibility_collection
                .in_set(VisibilitySystems::CheckVisibility),
        ),
    );

    let mut scene = World::new();
    scene.spawn(Mesh3d::default());
    let scene = app
        .world_mut()
        .resource_mut::<Assets<WorldAsset>>()
        .add(WorldAsset::new(scene));
    let model = app
        .world_mut()
        .spawn((
            PrefabModel {
                model: Default::default(),
                model_path: "fixture.gltf".into(),
                handle: None,
                scene_name: None,
                material_name: None,
            },
            PrefabShadowPolicy {
                casts: false,
                receives: false,
                visible_in_reflections: false,
            },
            RenderLayers::layer(29),
            ModelExpanded,
            WorldAssetRoot(scene.clone()),
        ))
        .id();

    app.update();
    let original_mesh = app
        .world_mut()
        .query_filtered::<Entity, With<Mesh3d>>()
        .single(app.world())
        .unwrap();

    // Use the dependency's actual instance-rebuild operation and ready event,
    // retaining the same model root and its completed presentation policy.
    app.world_mut()
        .resource_scope(|world, mut spawner: Mut<WorldInstanceSpawner>| {
            spawner.update_spawned_instances(world, &[scene.id()])
        })
        .unwrap();
    app.update();

    assert!(app.world().get_entity(original_mesh).is_err());
    let replacement_mesh = app
        .world_mut()
        .query_filtered::<Entity, With<Mesh3d>>()
        .single(app.world())
        .unwrap();
    assert_eq!(
        app.world()
            .get::<ChildOf>(replacement_mesh)
            .unwrap()
            .parent(),
        model
    );
}

fn assert_portrait_mesh_policy_at_visibility_collection(
    meshes: Query<(&RenderLayers, Has<NotShadowCaster>, Has<NotShadowReceiver>), With<Mesh3d>>,
) {
    let (layers, no_shadow_cast, no_shadow_receive) = meshes.single().unwrap();
    assert_eq!(layers, &RenderLayers::layer(29));
    assert!(no_shadow_cast);
    assert!(no_shadow_receive);
}
