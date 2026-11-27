use std::{io, path::Path};

use bevy::{
    asset::{AssetPath, LoadContext},
    gltf::Gltf,
};
use openzt2_game_data::{
    world_definitions::{
        document::WorldDefinitionDocument, extinct_animal_recovery::FossilSlotDefinition,
    },
    AssetId,
};

use crate::{
    asset_source::AssetArchives,
    model_source::native_model_source_lowering::native_model_scene_labelled_asset_path,
    scene_prefab::ScenePrefabAsset,
    source_document::ordered_source_document_types::OrderedSourceDocument,
    texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image,
    world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::DemandLoadedDependencyPath,
};

use super::{
    authored_binder_reference_resolution::AuthoredBinderSources,
    presentation_dependencies::resolve_world_definition_presentation_models,
    source_documents::read_source_document, unresolved_reference,
};

pub(super) async fn lower_fossil_puzzle_background_slots(
    archives: &AssetArchives,
    binders: &AuthoredBinderSources,
    context: &mut LoadContext<'_>,
    path: &str,
    documents: &[OrderedSourceDocument],
    document: &mut WorldDefinitionDocument,
) -> io::Result<Option<Vec<DemandLoadedDependencyPath>>> {
    let Some(puzzle) = documents.iter().find(|document| {
        document.path.key().eq_ignore_ascii_case(path)
            && document.root.name.eq_ignore_ascii_case("ZTPuzzle")
    }) else {
        return Ok(None);
    };
    let Some(animal) = puzzle.root.attribute_named_any(&["animal"]) else {
        return Ok(None);
    };
    let manager = read_source_document(context, "config/puzzlemgr.xml").await?;
    let entity_root = manager
        .root
        .attribute_named_any(&["entityRoot"])
        .ok_or_else(|| unresolved_reference(path, "fossil puzzle entity root", "entityRoot"))?;
    let mut textures = Vec::new();
    let binder = format!("{animal}Background");
    let source = binders
        .resolve_binder_type_source_path(archives, &binder)?
        .ok_or_else(|| unresolved_reference(path, "fossil puzzle background", &binder))?;
    let source = source.to_string_lossy();
    let background = read_source_document(context, &source).await?;
    let (_, models) =
        resolve_world_definition_presentation_models(archives, context, &[background]).await?;
    let mut models = models.values();
    let model = models
        .next()
        .filter(|first| models.all(|other| other == *first))
        .ok_or_else(|| unresolved_reference(path, "unambiguous fossil puzzle model", &binder))?;
    let scene_path = native_model_scene_labelled_asset_path(model);
    let scene_path = AssetPath::parse(&scene_path);
    let model = context
        .load_builder()
        .load_value::<Gltf>(scene_path.without_label())
        .await
        .map_err(io::Error::other)?;
    let background = scene_path
        .label()
        .and_then(|label| model.get_labeled(label))
        .and_then(|scene| scene.get::<ScenePrefabAsset>())
        .ok_or_else(|| unresolved_reference(path, "fossil puzzle scene", &binder))?;
    for piece in puzzle
        .root
        .element_children()
        .filter(|child| child.name.eq_ignore_ascii_case("pieces"))
        .flat_map(|group| group.element_children())
    {
        let node = piece
            .attribute_named_any(&["node"])
            .ok_or_else(|| unresolved_reference(path, "fossil puzzle piece node", &binder))?;
        let pose = background
            .prefab_attachment_transform(AssetId::from_key(&node.to_ascii_lowercase()))
            .ok_or_else(|| unresolved_reference(path, "fossil puzzle slot", node))?;
        let texture = piece
            .attribute_named_any(&["texture"])
            .ok_or_else(|| unresolved_reference(path, "fossil piece texture", node))?;
        let texture = format!("{entity_root}/{animal}/{texture}");
        let texture_path = archives
            .resolve_ui_reference(Path::new(path), &texture)
            .ok_or_else(|| unresolved_reference(path, "fossil piece texture", &texture))?;
        let texture_path = texture_path.to_string_lossy().replace('\\', "/");
        let id = AssetId::from_virtual_path(&texture_path);
        textures.push(DemandLoadedDependencyPath {
            id,
            path: select_bevy_image_asset_path_for_blue_fang_source_image(&texture_path)
                .into_boxed_str(),
        });
        let piece_id = AssetId::from_virtual_path(&format!("fossil-pieces/{animal}/{node}"));
        let definition = document
            .fossil_pieces
            .iter_mut()
            .find(|piece| piece.id == piece_id)
            .ok_or_else(|| unresolved_reference(path, "lowered fossil piece", node))?;
        definition.model = id;
        document.fossil_slots.push(FossilSlotDefinition {
            id: AssetId::from_virtual_path(&format!("fossil-slots/{animal}/{node}")),
            transform: pose.to_cols_array_2d(),
            tolerance_cm: 50,
            tolerance_degrees: 0,
        });
    }
    Ok(Some(textures))
}
