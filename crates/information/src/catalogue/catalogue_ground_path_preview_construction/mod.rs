use bevy::{camera::visibility::RenderLayers, prelude::*};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::paths_and_tile_surfaces::GuestPathDefinition;

use super::catalogue_preview_scene_projection::{
    CataloguePreviewPrefab, CATALOGUE_PREVIEW_SCENE_RENDER_LAYER,
};

pub(super) fn spawn_ground_path_catalogue_preview(
    commands: &mut Commands,
    parent: Entity,
    path: &GuestPathDefinition,
    definitions: WorldDefinitionsView<'_>,
    texture: Handle<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let half_width = f32::from(path.width_cm) * 0.01;
    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(half_width * 2.0, half_width * 2.0),
            ),
        ),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            unlit: true,
            ..default()
        })),
        Transform::IDENTITY,
        Visibility::Inherited,
        RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
        ChildOf(parent),
    ));
    let Some(curb) = definitions.find_fence(path.curb) else {
        return;
    };
    let Some(prefab) = definitions.scene(curb.segments.cardinal_straight) else {
        return;
    };
    for (x, z, heading) in [
        (-half_width, -half_width, 0.0),
        (half_width, -half_width, -std::f32::consts::FRAC_PI_2),
        (half_width, half_width, std::f32::consts::PI),
        (-half_width, half_width, std::f32::consts::FRAC_PI_2),
    ] {
        commands.spawn((
            CataloguePreviewPrefab::new(prefab.clone()),
            Transform::from_xyz(x, 0.0, z).with_rotation(Quat::from_rotation_y(heading)),
            Visibility::Inherited,
            RenderLayers::layer(CATALOGUE_PREVIEW_SCENE_RENDER_LAYER),
            ChildOf(parent),
        ));
    }
}
