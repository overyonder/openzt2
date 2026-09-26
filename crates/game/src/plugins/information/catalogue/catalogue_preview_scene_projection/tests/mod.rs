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
