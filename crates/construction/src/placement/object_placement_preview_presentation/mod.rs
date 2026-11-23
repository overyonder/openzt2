//! Builds authored placement decals and projects preview validity onto visuals.

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use bevy::prelude::*;
use openzt2_game_data::ui_document::document::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCell;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::placement::object_placement_geometry::{
    ScenePlacementFootprintChanged, ScenePlacementFootprints,
};
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;

use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::rotate_object_placement_point_by_quarter_turns;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementEntranceArrow as EntranceArrow;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewDecal;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRenderable;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_rectangular_surface_mesh_fitted_to_canonical_terrain_samples;

pub(super) fn hydrate_object_placement_preview_materials_from_ui_document(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<Ref<UiDocumentRoot>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut document_events: MessageReader<AssetEvent<UiDocumentAsset>>,
    mut palettes: Local<
        bevy::platform::collections::HashMap<
            bevy::asset::AssetId<UiDocumentAsset>,
            ObjectPlacementPreviewMaterials,
        >,
    >,
    mut previews: Query<
        (Entity, Option<&mut ObjectPlacementPreviewMaterials>),
        With<ConstructionPreview>,
    >,
) {
    for event in document_events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            palettes.remove(id);
        }
    }
    let Some((root, document)) = roots.iter().find_map(|root| {
        let document = documents.get(&root.document)?;
        matches!(
            &document.canonical_ui_document().role,
            UiDocumentRole::InGameHud
        )
        .then_some((root, document))
    }) else {
        return;
    };
    let id = root.document.id();
    let rebuild = !palettes.contains_key(&id);
    if rebuild {
        let policy = &document
            .canonical_ui_document()
            .construction_placement_preview;
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
        let (Some(footprint_valid), Some(footprint_invalid), grid_cardinal, grid_diagonal) =
            authored_material_handles
        else {
            return;
        };
        {
            let [Some(small_cardinal), Some(medium_cardinal), Some(large_cardinal)] = grid_cardinal
            else {
                return;
            };
            let [Some(small_diagonal), Some(medium_diagonal), Some(large_diagonal)] = grid_diagonal
            else {
                return;
            };
            palettes.insert(
                id,
                ObjectPlacementPreviewMaterials {
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
    let Some(palette) = palettes.get(&id) else {
        return;
    };
    for (entity, current) in &mut previews {
        if let Some(mut current) = current {
            if rebuild || root.is_changed() {
                *current = palette.clone();
            }
        } else {
            commands.entity(entity).insert(palette.clone());
        }
    }
}

const ENTRANCE_ARROW_IMAGE: &str = "ui/fx/footprint/entrance_arrow.tga";
// The pulse was matched by eye to the original.
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

/// The original game draws a building being placed without its alpha-blended meshes,
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

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PlacementDecalRole {
    Footprint(usize),
    Grid,
    EntranceArrow,
}

pub(super) type PlacedDecal<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlacementDecalRole,
        &'static mut Mesh3d,
        &'static mut MeshMaterial3d<StandardMaterial>,
        &'static mut Transform,
        Option<&'static mut EntranceArrow>,
    ),
>;

pub(super) fn rebuild_object_placement_preview_footprint_and_grid_decals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut plane: Local<Option<Handle<Mesh>>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut scene_changes: MessageReader<ScenePlacementFootprintChanged>,
    previews: Query<(
        Entity,
        Ref<ConstructionPreview>,
        &WorldMember,
        Ref<ObjectPlacementPreviewMaterials>,
        Option<Ref<ObjectPlacementPrefabSource>>,
        Ref<Visibility>,
        Option<&Children>,
    )>,
    mut decals: PlacedDecal,
    mut terrain: PreviewDecalTerrain,
    arrow_nodes: DockArrowNodes,
) {
    let changed_scenes = scene_changes
        .read()
        .map(|change| change.0)
        .collect::<bevy::platform::collections::HashSet<_>>();
    let definitions_changed = active_definitions.is_changed() || definitions.is_changed();
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let changed_models = arrow_nodes.changed_preview_models();
    let edited = !terrain.edits.is_empty();
    let restored = !terrain.restores.is_empty();
    terrain.edits.clear();
    terrain.restores.clear();
    for (entity, preview, member, materials, prefab, visibility, children) in &previews {
        let Ok(index) = terrain.indexes.get(member.root) else {
            continue;
        };
        if *visibility == Visibility::Hidden {
            continue;
        }
        let has_grid = children.is_some_and(|children| {
            children.iter().any(|child| {
                decals
                    .get(child)
                    .is_ok_and(|(role, ..)| *role == PlacementDecalRole::Grid)
            })
        });
        if has_grid
            && !edited
            && !restored
            && !index.is_changed()
            && !definitions_changed
            && !visibility.is_changed()
            && !preview.is_changed()
            && !materials.is_changed()
            && !changed_models.contains(&entity)
            && !prefab.as_ref().is_some_and(|source| {
                source.is_changed() || changed_scenes.contains(&source.0.id())
            })
        {
            continue;
        }
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
        let geometry = footprints.resolve(definition, prefab.as_deref().map(|source| &source.0));
        let footprint = geometry.cells(turns);
        let grid_size = match calculate_occupied_footprint_maximum_axis_span(footprint) {
            ..=2 => 0,
            3..=4 => 1,
            _ => 2,
        };
        let pivot_cm = geometry.pivot_cm(turns);
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
        let mut existing: bevy::platform::collections::HashMap<PlacementDecalRole, Entity> =
            children
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|child| decals.get(child).ok().map(|(role, ..)| (*role, child)))
                .collect();
        let plane =
            plane.get_or_insert_with(|| meshes.add(Plane3d::default().mesh().size(1.0, 1.0)));
        let projection = PreviewDecals {
            owner: entity,
            preview: preview.transform.compute_affine(),
            rotation: inverse_rotation,
            plane,
            terrain_index: &index,
        };
        let mut project =
            |role, translation, size, material: &Handle<StandardMaterial>, rotation| {
                projection.project(
                    &mut commands,
                    &mut meshes,
                    &terrain,
                    &mut decals,
                    existing.remove(&role),
                    role,
                    translation,
                    size,
                    material,
                    rotation,
                )
            };
        let rectangle = (turns & 1 == 0)
            .then(|| footprint_rectangle(footprint, pivot, turns, inverse_rotation))
            .flatten();
        if let Some((translation, size)) = rectangle {
            project(
                PlacementDecalRole::Footprint(0),
                translation,
                size,
                footprint_material,
                0.0,
            );
        } else {
            for (index, translation) in footprint_translations().enumerate() {
                project(
                    PlacementDecalRole::Footprint(index),
                    translation,
                    Vec2::ONE,
                    footprint_material,
                    0.0,
                );
            }
        }
        project(
            PlacementDecalRole::Grid,
            Vec3::new(0.0, 0.015, 0.0),
            Vec2::splat(materials.grid_radius[grid_size] * 2.0),
            grid_material,
            0.0,
        );
        let arrow = definition.dock_arrow.and_then(|arrow| {
            let (low, high) = arrow_nodes.model_bounds(entity)?;
            let node = arrow_nodes.dock_arrow(entity)?;
            let inward = (-node).with_y(0.0).normalize_or_zero();
            let rotation = preview.transform.rotation;
            let occupied = footprint_translations().collect::<Vec<_>>();
            let inside = |point: Vec3| {
                (point.x > low.x && point.x < high.x && point.z > low.z && point.z < high.z)
                    || occupied.iter().any(|cell| {
                        let offset = rotation * (point - *cell);
                        offset.x.abs() < 0.5 && offset.z.abs() < 0.5
                    })
            };
            let edge = (0_u8..80)
                .map(|step| node - inward * (f32::from(step) * 0.25))
                .find(|point| !inside(point.with_y(0.025)))?;
            let rest = (edge - inward * (arrow.size_metres * 0.5)).with_y(0.03);
            Some((rest, inward, arrow.size_metres))
        });
        if let Some((rest, direction, size)) = arrow {
            let world_direction = preview.transform.rotation.mul_vec3(direction);
            let entity = project(
                PlacementDecalRole::EntranceArrow,
                rest,
                Vec2::splat(size),
                &materials.entrance_arrow,
                world_direction.x.atan2(world_direction.z),
            );
            if let Ok((.., Some(mut arrow))) = decals.get_mut(entity) {
                if arrow.rest != rest || arrow.direction != direction {
                    *arrow = EntranceArrow { rest, direction };
                }
            } else {
                commands
                    .entity(entity)
                    .insert(EntranceArrow { rest, direction });
            }
        }
        for obsolete in existing.into_values() {
            commands.entity(obsolete).try_despawn();
        }
    }
}

pub(super) fn project_object_placement_validity_onto_preview_model_tint(
    previews: Query<(
        Entity,
        &ConstructionPreview,
        &ObjectPlacementPreviewMaterials,
    )>,
    children: Query<&Children>,
    mut renderables: Query<&mut PrefabModelTint, With<ObjectPlacementPreviewRenderable>>,
) {
    for (entity, preview, materials) in &previews {
        let next = if matches!(preview.validity, PlacementValidity::Valid { .. }) {
            materials.model_valid
        } else {
            materials.model_invalid
        };
        for descendant in children.iter_descendants_depth_first::<Children>(entity) {
            let Ok(mut tint) = renderables.get_mut(descendant) else {
                continue;
            };
            if tint.0 != next {
                tint.0 = next;
            }
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

/// The models' `Dock_Arrow` nodes, where the original game stands a `special` arrow.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct DockArrowNodes<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    // Local transforms, as a freshly loaded model's global ones lag a frame.
    transforms: Query<'w, 's, (&'static Transform, &'static ChildOf), Without<PlacementDecalRole>>,
    bounds: Query<'w, 's, &'static bevy::camera::primitives::Aabb, Without<ObjectPlacementPreviewDecal>>,
    nodes: Query<
        'w,
        's,
        &'static openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    previews: Query<'w, 's, (), With<ConstructionPreview>>,
    changed: Query<'w, 's, Entity, (
        Without<ObjectPlacementPreviewDecal>,
        Or<(
            Added<bevy::camera::primitives::Aabb>,
            Added<openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier>,
        )>,
    )>,
}

impl DockArrowNodes<'_, '_> {
    fn changed_preview_models(&self) -> bevy::ecs::entity::EntityHashSet {
        self.changed
            .iter()
            .filter_map(|entity| {
                self.parents
                    .iter_ancestors::<ChildOf>(entity)
                    .find(|&owner| self.previews.contains(owner))
            })
            .collect()
    }

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
    /// the original game draws one arrow, on the `Dock_Arrow1` side.
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
    edits: MessageReader<'w, 's, openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>,
    restores: MessageReader<
        'w,
        's,
        openzt2_zoo_state::terrain::terrain_edit_types::TerrainPresentationRestored,
    >,
    indexes:
        Query<'w, 's, Ref<'static, openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>>,
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
    terrain_index: &'a openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex,
}

impl PreviewDecals<'_> {
    #[allow(clippy::too_many_arguments)]
    fn project(
        &self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        terrain: &PreviewDecalTerrain,
        decals: &mut PlacedDecal,
        existing: Option<Entity>,
        role: PlacementDecalRole,
        translation: Vec3,
        size: Vec2,
        material: &Handle<StandardMaterial>,
        texture_rotation: f32,
    ) -> Entity {
        let centre = self.preview.transform_point3(translation);
        let fitted = construct_rectangular_surface_mesh_fitted_to_canonical_terrain_samples(
            size,
            centre,
            texture_rotation,
            self.terrain_index,
            &terrain.assets,
            &terrain.chunks,
        );
        let scale = if fitted.is_some() {
            Vec3::ONE
        } else {
            Vec3::new(size.x, 1.0, size.y)
        };
        let transform = Transform::from_translation(translation)
            .with_rotation(self.rotation)
            .with_scale(scale);
        if let Some(entity) = existing {
            if let Ok((_, mut mesh, mut current_material, mut pose, _)) = decals.get_mut(entity) {
                match fitted {
                    Some(next) => {
                        if mesh.0 != *self.plane && meshes.contains(mesh.0.id()) {
                            if let Some(existing_mesh) = meshes.get_mut(&mesh.0) {
                                *existing_mesh = next;
                            }
                        } else {
                            mesh.0 = meshes.add(next);
                        }
                    }
                    None => {
                        mesh.set_if_neq(Mesh3d(self.plane.clone()));
                    }
                }
                current_material.set_if_neq(MeshMaterial3d(material.clone()));
                pose.set_if_neq(transform);
                return entity;
            }
        }
        commands
            .spawn((
                Mesh3d(fitted.map_or_else(|| self.plane.clone(), |mesh| meshes.add(mesh))),
                MeshMaterial3d(material.clone()),
                transform,
                role,
                ObjectPlacementPreviewDecal,
                ChildOf(self.owner),
                bevy::camera::visibility::RenderLayers::layer(
                    openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER,
                ),
            ))
            .id()
    }
}
