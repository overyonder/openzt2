use super::*;

#[test]
fn missing_registry_does_not_retain_a_cleared_or_replaced_preview() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<SelectedCatalogueEntry>()
        .add_systems(Update, project_selected_catalogue_entry_into_preview_scene);
    let owner = app.world_mut().spawn_empty().id();
    for selection in [None, Some(AssetId([2; 16]))] {
        app.world_mut().resource_mut::<SelectedCatalogueEntry>().0 = selection;
        let scene = app.world_mut().spawn_empty().id();
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let node = app
            .world_mut()
            .spawn((
                UiDocumentOwner(owner),
                UiImageBinding(UiImagePropertyBindingSource::CatalogueEntryPreview),
                InheritedVisibility::VISIBLE,
                ImageNode::new(image),
                CataloguePreviewScene {
                    definition: AssetId([1; 16]),
                    scene,
                    catalogue_revision: 0,
                },
            ))
            .id();
        app.update();
        assert!(app.world().get_entity(scene).is_err());
        assert!(app.world().get::<CataloguePreviewScene>(node).is_none());
        assert_eq!(
            app.world().get::<ImageNode>(node).unwrap().image,
            Handle::default()
        );
        app.world_mut().entity_mut(node).despawn();
    }
}

#[test]
fn removed_ui_owner_retires_one_portrait_hierarchy_and_preserves_other_camera_owners() {
    use openzt2_world_state::world_spawn::world_unloading::assert_no_invalid_entity_commands_during;

    let mut app = App::new();
    app.add_systems(
        Update,
        (
            synchronize_catalogue_preview_camera_activity_with_authored_image_visibility,
            retire_catalogue_preview_scenes_without_ui_owners,
        )
            .chain(),
    )
    .edit_schedule(Update, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    let removed_owner = app.world_mut().spawn(InheritedVisibility::VISIBLE).id();
    let retained_owner = app.world_mut().spawn(InheritedVisibility::VISIBLE).id();
    let disabled_owner = app.world_mut().spawn(Disabled).id();
    let mut scenes = Vec::new();
    let mut cameras = Vec::new();
    for owner in [removed_owner, retained_owner, disabled_owner] {
        let scene = app.world_mut().spawn(CataloguePreviewOwner(owner)).id();
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    is_active: false,
                    ..default()
                },
                CataloguePreviewOwner(owner),
                ChildOf(scene),
            ))
            .id();
        app.world_mut()
            .entity_mut(owner)
            .insert(CataloguePreviewScene {
                definition: AssetId([1; 16]),
                catalogue_revision: 0,
                scene,
            });
        scenes.push(scene);
        cameras.push(camera);
    }
    let prefab = app.world_mut().spawn(ChildOf(scenes[0])).id();
    let renderer = app.world_mut().spawn((ChildOf(prefab), Disabled)).id();
    assert_no_invalid_entity_commands_during(|| app.update());
    assert!(app.world().get::<Camera>(cameras[0]).unwrap().is_active);
    assert!(app.world().get::<Camera>(cameras[1]).unwrap().is_active);

    app.world_mut().entity_mut(removed_owner).despawn();
    assert_no_invalid_entity_commands_during(|| app.update());
    for entity in [scenes[0], cameras[0], prefab, renderer] {
        assert!(app.world().get_entity(entity).is_err(), "retained {entity}");
    }
    for index in [1, 2] {
        assert!(app.world().get_entity(scenes[index]).is_ok());
        assert_eq!(
            app.world()
                .get::<CataloguePreviewOwner>(cameras[index])
                .unwrap()
                .0,
            [retained_owner, disabled_owner][index - 1]
        );
    }
    assert!(app.world().get::<Camera>(cameras[1]).unwrap().is_active);
    assert_no_invalid_entity_commands_during(|| app.update());
}
