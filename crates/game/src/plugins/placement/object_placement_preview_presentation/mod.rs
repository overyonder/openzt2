//! Builds authored placement decals and projects preview validity onto visuals.

use bevy::{prelude::*, render::render_resource::Face};
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::world_definitions::object_placement::{FootprintCell, FootprintCellFlags};
use openzt2_game_data::AssetId;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::world_spawn::prefab_model_tint::PrefabModelTint;

use super::{
    object_placement_definition_queries::{
        resolve_object_placeable_definition, select_authored_footprint_for_eighth_turns,
    },
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        rotate_object_placement_point_by_quarter_turns,
    },
    ObjectPlacementPreviewDecal, ObjectPlacementPreviewMaterials, ObjectPlacementPreviewOwner,
    ObjectPlacementPreviewRenderable, ObjectPlacementPreviewVisualState,
};

pub(super) fn hydrate_object_placement_preview_materials_from_ui_document(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    previews: Query<
        Entity,
        (
            With<ConstructionPreview>,
            Without<ObjectPlacementPreviewMaterials>,
        ),
    >,
) {
    let Some(document) = roots.iter().find_map(|root| {
        let document = documents.get(&root.document)?;
        matches!(
            &document.canonical_ui_document().role,
            UiDocumentRole::InGameHud
        )
        .then_some(document)
    }) else {
        return;
    };
    let policy = &document
        .canonical_ui_document()
        .construction_placement_preview;
    for entity in &previews {
        let create_authored_unlit_preview_material =
            |texture, color, materials: &mut Assets<StandardMaterial>| {
                document.cloned_texture_image_handle(texture).map(|image| {
                    materials.add(StandardMaterial {
                        base_color: convert_authored_srgba_bytes_to_bevy_color(color),
                        base_color_texture: Some(image),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        cull_mode: Some(Face::Back),
                        ..default()
                    })
                })
            };
        let authored_material_handles = (
            create_authored_unlit_preview_material(
                AssetId(policy.footprint_valid.0),
                policy.footprint_valid_srgba,
                &mut materials,
            ),
            create_authored_unlit_preview_material(
                AssetId(policy.footprint_invalid.0),
                policy.footprint_invalid_srgba,
                &mut materials,
            ),
            [
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_small_cardinal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_medium_cardinal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_large_cardinal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
            ],
            [
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_small_diagonal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_medium_diagonal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
                create_authored_unlit_preview_material(
                    AssetId(policy.grid_large_diagonal.0),
                    policy.grid_srgba,
                    &mut materials,
                ),
            ],
        );
        if let (Some(footprint_valid), Some(footprint_invalid), grid_cardinal, grid_diagonal) =
            authored_material_handles
        {
            let [Some(small_cardinal), Some(medium_cardinal), Some(large_cardinal)] = grid_cardinal
            else {
                continue;
            };
            let [Some(small_diagonal), Some(medium_diagonal), Some(large_diagonal)] = grid_diagonal
            else {
                continue;
            };
            commands
                .entity(entity)
                .insert(ObjectPlacementPreviewMaterials {
                    footprint_valid,
                    footprint_invalid,
                    grid_cardinal: [small_cardinal, medium_cardinal, large_cardinal],
                    grid_diagonal: [small_diagonal, medium_diagonal, large_diagonal],
                    grid_radius: [
                        policy.grid_small_radius,
                        policy.grid_medium_radius,
                        policy.grid_large_radius,
                    ],
                    model_valid: if policy.model_valid_srgba[..3] == [0, 0, 0] {
                        Color::WHITE
                    } else {
                        convert_authored_srgba_bytes_to_bevy_color(policy.model_valid_srgba)
                    },
                    model_invalid: convert_authored_srgba_bytes_to_bevy_color(
                        policy.model_invalid_srgba,
                    ),
                });
        }
    }
}

pub(super) fn rebuild_object_placement_preview_footprint_and_grid_decals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut plane: Local<Option<Handle<Mesh>>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut previews: Query<(
        Entity,
        &ConstructionPreview,
        &ObjectPlacementPreviewMaterials,
        Option<&mut ObjectPlacementPreviewVisualState>,
    )>,
    decals: Query<(Entity, &ObjectPlacementPreviewOwner), With<ObjectPlacementPreviewDecal>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, preview, materials, state) in &mut previews {
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            continue;
        };
        let Some(turns) =
            calculate_authored_object_placement_eighth_turns(definition, &preview.transform)
        else {
            continue;
        };
        let valid = matches!(preview.validity, PlacementValidity::Valid { .. });
        let footprint = select_authored_footprint_for_eighth_turns(definition, turns);
        let grid_size = match calculate_occupied_footprint_maximum_axis_span(footprint) {
            ..=2 => 0,
            3..=4 => 1,
            _ => 2,
        };
        let pivot_cm = if turns & 1 == 0 {
            &definition.pivot_cm
        } else {
            &definition.diagonal_pivot_cm
        };
        let pivot = Vec2::new(f32::from(pivot_cm[0]), f32::from(pivot_cm[1])) / 100.0;
        let inverse_rotation = preview.transform.rotation.inverse();
        let footprint_material = if valid {
            &materials.footprint_valid
        } else {
            &materials.footprint_invalid
        };
        let grid_material = if turns & 1 == 0 {
            &materials.grid_cardinal[grid_size]
        } else {
            &materials.grid_diagonal[grid_size]
        };
        let grid_radius = materials.grid_radius[grid_size];
        let footprint_translations = || {
            footprint
                .iter()
                .filter(|cell| footprint_cell_is_occupied(cell))
                .map(move |cell| {
                    let local = IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1]));
                    let world_offset = rotate_object_placement_point_by_quarter_turns(
                        local.as_vec2() + Vec2::splat(0.5) - pivot,
                        turns / 2,
                    );
                    inverse_rotation.mul_vec3(Vec3::new(world_offset.x, 0.025, world_offset.y))
                })
        };
        // Compare the emitted draw inputs, not the change tick of the entire
        // definition catalogue. Unchanged previews allocate no new projection.
        if state.as_deref().is_some_and(|state| {
            state.eighth_turns == turns
                && state.valid == valid
                && state.decal_rotation == inverse_rotation
                && state.footprint_material == *footprint_material
                && state.grid_material == *grid_material
                && state.grid_radius == grid_radius
                && state
                    .footprint_translations
                    .iter()
                    .copied()
                    .eq(footprint_translations())
        }) {
            continue;
        }
        let next = ObjectPlacementPreviewVisualState {
            eighth_turns: turns,
            valid,
            footprint_translations: footprint_translations().collect(),
            decal_rotation: inverse_rotation,
            footprint_material: footprint_material.clone(),
            grid_material: grid_material.clone(),
            grid_radius,
        };
        let plane = plane
            .get_or_insert_with(|| meshes.add(Plane3d::default().mesh().size(1.0, 1.0)))
            .clone();
        for (decal, owner) in &decals {
            if owner.0 == entity {
                commands.entity(decal).despawn();
            }
        }
        for translation in &next.footprint_translations {
            commands.spawn((
                Mesh3d(plane.clone()),
                MeshMaterial3d(next.footprint_material.clone()),
                Transform::from_translation(*translation).with_rotation(next.decal_rotation),
                ObjectPlacementPreviewDecal,
                ObjectPlacementPreviewOwner(entity),
                ChildOf(entity),
            ));
        }
        commands.spawn((
            Mesh3d(plane),
            MeshMaterial3d(next.grid_material.clone()),
            Transform::from_xyz(0.0, 0.015, 0.0)
                .with_rotation(next.decal_rotation)
                .with_scale(Vec3::new(
                    next.grid_radius * 2.0,
                    1.0,
                    next.grid_radius * 2.0,
                )),
            ObjectPlacementPreviewDecal,
            ObjectPlacementPreviewOwner(entity),
            ChildOf(entity),
        ));
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

pub(super) fn project_object_placement_validity_onto_preview_model_tint(
    previews: Query<Ref<ConstructionPreview>>,
    materials: Query<&ObjectPlacementPreviewMaterials>,
    mut renderables: Query<
        (&ObjectPlacementPreviewOwner, &mut PrefabModelTint),
        With<ObjectPlacementPreviewRenderable>,
    >,
) {
    for (owner, mut tint) in &mut renderables {
        let (Ok(preview), Ok(materials)) = (previews.get(owner.0), materials.get(owner.0)) else {
            continue;
        };
        if !preview.is_changed() && !tint.is_added() {
            continue;
        }
        let next = if matches!(preview.validity, PlacementValidity::Valid { .. }) {
            materials.model_valid
        } else {
            materials.model_invalid
        };
        if tint.0 != next {
            tint.0 = next;
        }
    }
}

fn footprint_cell_is_occupied(cell: &&FootprintCell) -> bool {
    cell.flags.contains_all(FootprintCellFlags::OCCUPIED)
}

fn calculate_occupied_footprint_maximum_axis_span(cells: &[FootprintCell]) -> i32 {
    let mut occupied_cells = cells
        .iter()
        .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED));
    let Some(first_cell) = occupied_cells.next() else {
        return 0;
    };
    let first_x = i32::from(first_cell.offset[0]);
    let first_y = i32::from(first_cell.offset[1]);
    let (minimum_x, maximum_x, minimum_y, maximum_y) = occupied_cells.fold(
        (first_x, first_x, first_y, first_y),
        |(minimum_x, maximum_x, minimum_y, maximum_y), cell| {
            let x = i32::from(cell.offset[0]);
            let y = i32::from(cell.offset[1]);
            (
                minimum_x.min(x),
                maximum_x.max(x),
                minimum_y.min(y),
                maximum_y.max(y),
            )
        },
    );
    (maximum_x - minimum_x + 1).max(maximum_y - minimum_y + 1)
}

fn convert_authored_srgba_bytes_to_bevy_color(value: [u8; 4]) -> Color {
    Color::srgba_u8(value[0], value[1], value[2], value[3])
}
