//! Builds authored placement decals and projects preview validity onto visuals.

use bevy::prelude::*;
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCell;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_game_data::AssetId;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;

use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_definition_queries::select_authored_footprint_for_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::rotate_object_placement_point_by_quarter_turns;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewDecal;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewOwner;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRenderable;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewVisualState;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_rectangular_surface_mesh_fitted_to_canonical_terrain_samples;

pub(super) fn hydrate_object_placement_preview_materials_from_ui_document(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
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
                        // Terrain-fitted decals may wind either way.
                        cull_mode: None,
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
                    // As a footprint, the grid's radius counts the
                    // original's 2 m tiles: its texture has a line every tile.
                    grid_radius: [
                        policy.grid_small_radius,
                        policy.grid_medium_radius,
                        policy.grid_large_radius,
                    ]
                    .map(|tiles| tiles * 2.0),
                    entrance_arrow: materials.add(StandardMaterial {
                        base_color_texture: Some(asset_server.load(ENTRANCE_ARROW_IMAGE)),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        cull_mode: None,
                        ..default()
                    }),
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

const ENTRANCE_ARROW_IMAGE: &str = "ui/fx/footprint/entrance_arrow.tga";
// ponytail: the pulse was matched by eye to the original.
const ENTRANCE_ARROW_PULSE_M: f32 = 0.3;
const ENTRANCE_ARROW_PULSE_SECONDS: f32 = 1.0;

/// The world-axis rectangle a cardinal footprint covers: its centre in the
/// preview's frame and its size.
fn footprint_rectangle(
    footprint: &[FootprintCell],
    pivot: Vec2,
    turns: u8,
    inverse_rotation: Quat,
) -> Option<(Vec3, Vec2)> {
    let (minimum, maximum) = footprint
        .iter()
        .filter(footprint_cell_is_occupied)
        .map(|cell| IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1])))
        .fold(None, |bounds: Option<(IVec2, IVec2)>, cell| {
            Some(bounds.map_or((cell, cell), |(low, high)| (low.min(cell), high.max(cell))))
        })?;
    let size = (maximum - minimum + IVec2::ONE).as_vec2();
    let centre = rotate_object_placement_point_by_quarter_turns(
        minimum.as_vec2() + size * 0.5 - pivot,
        turns / 2,
    );
    let size = if (turns / 2) % 2 == 1 {
        size.yx()
    } else {
        size
    };
    Some((
        inverse_rotation.mul_vec3(Vec3::new(centre.x, 0.025, centre.y)),
        size,
    ))
}

/// `zt.exe` draws a building being placed without its alpha-blended meshes,
/// such as a station's glass and the track under it.
pub(super) fn hide_alpha_blended_meshes_of_object_placement_previews(
    materials: Res<
        Assets<openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial>,
    >,
    previews: Query<(), With<ConstructionPreview>>,
    parents: Query<&ChildOf>,
    mut meshes: Query<
        (
            Entity,
            &MeshMaterial3d<openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial>,
            &openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder,
            &mut Visibility,
        ),
        Changed<MeshMaterial3d<openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial>>,
    >,
) {
    for (entity, material, order, mut visibility) in &mut meshes {
        // A technique's later passes hang under its first and hide with it.
        if order.is_first_authored_pass()
            && materials
                .get(&material.0)
                .is_some_and(|material| !material.has_opaque_blend_state())
            && parents
                .iter_ancestors(entity)
                .any(|ancestor| previews.contains(ancestor))
        {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Slides each entrance arrow back and forth along the way it points.
pub(super) fn pulse_object_placement_entrance_arrows(
    time: Res<Time>,
    mut arrows: Query<(
        &mut Transform,
        &openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementEntranceArrow,
    )>,
) {
    let phase = time.elapsed_secs() / ENTRANCE_ARROW_PULSE_SECONDS * core::f32::consts::TAU;
    for (mut transform, arrow) in &mut arrows {
        transform.translation =
            arrow.rest + arrow.direction * (ENTRANCE_ARROW_PULSE_M * phase.sin());
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
    terrain: PreviewDecalTerrain,
    arrow_nodes: DockArrowNodes,
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
                && state.world_translation == preview.transform.translation
                && !state.arrow_pending
                && state
                    .footprint_translations
                    .iter()
                    .copied()
                    .eq(footprint_translations())
        }) {
            continue;
        }
        let mut next = ObjectPlacementPreviewVisualState {
            eighth_turns: turns,
            valid,
            footprint_translations: footprint_translations().collect(),
            decal_rotation: inverse_rotation,
            footprint_material: footprint_material.clone(),
            grid_material: grid_material.clone(),
            grid_radius,
            world_translation: preview.transform.translation,
            arrow_pending: false,
        };
        let plane = plane
            .get_or_insert_with(|| meshes.add(Plane3d::default().mesh().size(1.0, 1.0)))
            .clone();
        for (decal, owner) in &decals {
            if owner.0 == entity {
                commands.entity(decal).despawn();
            }
        }
        let decals = PreviewDecals {
            owner: entity,
            preview: preview.transform.compute_affine(),
            rotation: next.decal_rotation,
            plane: &plane,
        };
        // As the original, one footprint image spans the whole footprint, so
        // its border outlines the footprint rather than every cell.
        // ponytail: a diagonal footprint keeps one image per cell; span it
        // with a rotated rectangle if the diagonal outline matters.
        let rectangle = (turns & 1 == 0)
            .then(|| footprint_rectangle(footprint, pivot, turns, inverse_rotation))
            .flatten();
        if let Some((translation, size)) = rectangle {
            decals.spawn_turned(
                &mut commands,
                &mut meshes,
                &terrain,
                translation,
                size,
                &next.footprint_material,
                0.0,
            );
        } else {
            for translation in &next.footprint_translations {
                decals.spawn(
                    &mut commands,
                    &mut meshes,
                    &terrain,
                    *translation,
                    1.0,
                    &next.footprint_material,
                );
            }
        }
        decals.spawn(
            &mut commands,
            &mut meshes,
            &terrain,
            Vec3::new(0.0, 0.015, 0.0),
            next.grid_radius * 2.0,
            &next.grid_material,
        );
        // A model loads after its preview: until its meshes are bounded, a
        // `special` arrow waits and the decals are laid again.
        let mut arrow_pending = false;
        let arrow = definition.dock_arrow.and_then(|arrow| {
            let Some((low, high)) = arrow_nodes.model_bounds(entity) else {
                arrow_pending = true;
                return None;
            };
            // The arrow stands at the model's Dock_Arrow node, just outside
            // the footprint and the model there so a roof doesn't hide it,
            // and points into the building.
            let node = arrow_nodes.dock_arrow(entity)?;
            let inward = (-node).with_y(0.0).normalize_or_zero();
            let rotation = preview.transform.rotation;
            let inside = |point: Vec3| {
                (point.x > low.x && point.x < high.x && point.z > low.z && point.z < high.z)
                    || next.footprint_translations.iter().any(|cell| {
                        let offset = rotation * (point - *cell);
                        offset.x.abs() < 0.5 && offset.z.abs() < 0.5
                    })
            };
            let edge = (0_u8..80)
                .map(|step| node - inward * (f32::from(step) * 0.25))
                .find(|point| !inside(point.with_y(0.025)))?;
            let rest = (edge - inward * (arrow.size_metres * 0.5)).with_y(0.03);
            Some(((rest, inward), arrow.size_metres))
        });
        if let Some(((rest, direction), size)) = arrow {
            // The texture points down its rows; turn it to point inward.
            let world_direction = preview.transform.rotation.mul_vec3(direction);
            let arrow = decals.spawn_turned(
                &mut commands,
                &mut meshes,
                &terrain,
                rest,
                Vec2::splat(size),
                &materials.entrance_arrow,
                world_direction.x.atan2(world_direction.z),
            );
            commands.entity(arrow).insert(
                openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementEntranceArrow {
                    rest,
                    direction,
                },
            );
        }
        next.arrow_pending = arrow_pending;
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

/// The models' `Dock_Arrow` nodes, where `zt.exe` stands a `special` arrow.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct DockArrowNodes<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    // Local transforms, as a freshly loaded model's global ones lag a frame.
    transforms: Query<'w, 's, (&'static Transform, &'static ChildOf)>,
    bounds: Query<'w, 's, &'static bevy::camera::primitives::Aabb, Without<ObjectPlacementPreviewDecal>>,
    nodes: Query<
        'w,
        's,
        &'static openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    >,
}

impl DockArrowNodes<'_, '_> {
    /// `node`'s frame within the preview's, without the presentation's lag.
    fn in_preview(&self, preview: Entity, mut node: Entity) -> Option<bevy::math::Affine3A> {
        let mut affine = bevy::math::Affine3A::IDENTITY;
        while node != preview {
            let (transform, parent) = self.transforms.get(node).ok()?;
            // The model's presentation trails the preview while the pointer
            // moves; the arrow stands where the preview is.
            if parent.parent() == preview {
                break;
            }
            affine = transform.compute_affine() * affine;
            node = parent.parent();
        }
        Some(affine)
    }

    /// The model's bounds in the preview's frame, once its meshes are bounded.
    fn model_bounds(&self, preview: Entity) -> Option<(Vec3, Vec3)> {
        self.children
            .iter_descendants(preview)
            .filter_map(|node| Some((self.in_preview(preview, node)?, self.bounds.get(node).ok()?)))
            .flat_map(|(affine, aabb)| {
                let (centre, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
                [-1.0_f32, 1.0].into_iter().flat_map(move |x| {
                    [-1.0_f32, 1.0]
                        .map(move |z| affine.transform_point3(centre + half * Vec3::new(x, 0.0, z)))
                })
            })
            .fold(None, |bounds, corner| {
                Some(
                    bounds.map_or((corner, corner), |(low, high): (Vec3, Vec3)| {
                        (low.min(corner), high.max(corner))
                    }),
                )
            })
    }

    /// A station has `Dock_Arrow` and `Dock_Arrow1` on opposite sides;
    /// `zt.exe` draws one arrow, on the `Dock_Arrow1` side.
    fn dock_arrow(&self, preview: Entity) -> Option<Vec3> {
        let find = |key| {
            let wanted = AssetId::from_key(key);
            let node = self
                .children
                .iter_descendants(preview)
                .find(|node| self.nodes.get(*node).is_ok_and(|name| name.0 == wanted))?;
            Some(self.in_preview(preview, node)?.translation.into())
        };
        find("dock_arrow1").or_else(|| find("dock_arrow"))
    }
}

/// The ground placement decals are fitted to.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PreviewDecalTerrain<'w, 's> {
    index: Res<'w, openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>,
    assets: Res<
        'w,
        Assets<
            openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
        >,
    >,
    chunks: Query<
        'w,
        's,
        (
            &'static openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk,
            Option<&'static openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples>,
        ),
    >,
}

struct PreviewDecals<'a> {
    owner: Entity,
    preview: bevy::math::Affine3A,
    rotation: Quat,
    plane: &'a Handle<Mesh>,
}

impl PreviewDecals<'_> {
    /// A decal fitted to the terrain, as the original draws it on the ground,
    /// so a slope doesn't bury it; a flat plane stands in off the map.
    fn spawn(
        &self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        terrain: &PreviewDecalTerrain,
        translation: Vec3,
        width: f32,
        material: &Handle<StandardMaterial>,
    ) {
        self.spawn_turned(
            commands,
            meshes,
            terrain,
            translation,
            Vec2::splat(width),
            material,
            0.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_turned(
        &self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        terrain: &PreviewDecalTerrain,
        translation: Vec3,
        size: Vec2,
        material: &Handle<StandardMaterial>,
        texture_rotation: f32,
    ) -> Entity {
        let centre = self.preview.transform_point3(translation);
        let (mesh, scale) = construct_rectangular_surface_mesh_fitted_to_canonical_terrain_samples(
            size,
            centre,
            texture_rotation,
            &terrain.index,
            &terrain.assets,
            &terrain.chunks,
        )
        .map_or_else(
            || (self.plane.clone(), Vec3::new(size.x, 1.0, size.y)),
            |mesh| (meshes.add(mesh), Vec3::ONE),
        );
        commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(translation)
                    .with_rotation(self.rotation)
                    .with_scale(scale),
                ObjectPlacementPreviewDecal,
                ObjectPlacementPreviewOwner(self.owner),
                ChildOf(self.owner),
                // Drawn with the terrain, as the fence grid is; otherwise the
                // terrain pass covers it.
                bevy::camera::visibility::RenderLayers::layer(
                    openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER,
                ),
            ))
            .id()
    }
}
