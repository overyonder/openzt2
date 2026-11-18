use super::*;
use crate::model_render::prefab_model_level_of_detail_projection::project_prefab_model_level_of_detail_visibility;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange;

fn spawn_lod(world: &mut World, parent: Entity) -> Entity {
    let group = world
        .spawn((ChildOf(parent), GlobalTransform::IDENTITY))
        .id();
    world
        .spawn((
            ChildOf(group),
            GlobalTransform::IDENTITY,
            Visibility::Hidden,
            PrefabModelLevelOfDetailVisibilityRange {
                group,
                ordinal: 0,
                center_m: Vec3::NEG_Z,
                near_m: 0.0,
                far_m: 10.0,
                active_without_range: false,
            },
        ))
        .id()
}

#[test]
fn removing_an_inherited_layer_reselects_only_its_lod_group() {
    let mut app = App::new();
    register_inherited_render_layers(&mut app);
    app.init_resource::<GraphicsSettings>().add_systems(
        PostUpdate,
        project_prefab_model_level_of_detail_visibility
            .in_set(ModelRenderPresentationSet::LevelOfDetail),
    );
    let world = app.world_mut();
    world.spawn((Camera3d::default(), GlobalTransform::IDENTITY));
    let changed_root = world.spawn(RenderLayers::layer(1)).id();
    let unaffected_root = world.spawn_empty().id();
    let changed_member = spawn_lod(world, changed_root);
    let unaffected_member = spawn_lod(world, unaffected_root);
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(changed_member),
        Some(&Visibility::Hidden)
    );
    assert_eq!(
        app.world().get::<Visibility>(unaffected_member),
        Some(&Visibility::Inherited)
    );

    app.world_mut()
        .entity_mut(unaffected_member)
        .insert(Visibility::Hidden);
    app.add_systems(
        PostUpdate,
        (move |mut commands: Commands| {
            commands.entity(changed_root).remove::<RenderLayers>();
        })
        .before(project_prefab_camera_layer_sources),
    );
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(changed_member),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world().get::<Visibility>(unaffected_member),
        Some(&Visibility::Hidden)
    );
}
