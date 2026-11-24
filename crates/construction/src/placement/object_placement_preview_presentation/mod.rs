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

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
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
                },
            );
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
const ENTRANCE_ARROW_RADIANS_PER_SECOND: f32 = 6.0;

#[derive(Component)]
pub(super) struct EntranceArrow {
    socket: Vec3,
    outward: Vec3,
    radius: f32,
    phase: f32,
}

impl EntranceArrow {
    fn translation(&self) -> Vec3 {
        self.socket + self.outward * (self.radius + (self.phase.cos() + 1.0) * 0.5)
    }
}

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
    mut arrows: Query<(&mut Transform, &mut EntranceArrow)>,
) {
    for (mut transform, mut arrow) in &mut arrows {
        arrow.phase += time.delta_secs() * ENTRANCE_ARROW_RADIANS_PER_SECOND;
        if arrow.phase > core::f32::consts::TAU {
            arrow.phase = 0.0;
        }
        transform
            .map_unchanged(|pose| &mut pose.translation)
            .set_if_neq(arrow.translation());
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PlacementDecalRole {
    Footprint,
    Grid,
    EntranceArrow(usize),
}

type PlacedDecal<'w, 's> = Query<
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
        &ConstructionPreview,
        Ref<Transform>,
        &WorldMember,
        Ref<ObjectPlacementPreviewMaterials>,
        Option<Ref<ObjectPlacementPrefabSource>>,
        Ref<Visibility>,
        Option<&Children>,
    )>,
    mut decals: PlacedDecal,
    mut terrain: PreviewDecalTerrain,
    prefab_assets: Res<Assets<ScenePrefabAsset>>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
) {
    let changed_scenes = scene_changes
        .read()
        .map(|change| change.0)
        .collect::<bevy::platform::collections::HashSet<_>>();
    let definitions_changed = active_definitions.is_changed() || definitions.is_changed();
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let changed_prefabs = prefab_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(*id)
            }
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    let edited = !terrain.edits.is_empty();
    let restored = !terrain.restores.is_empty();
    terrain.edits.clear();
    terrain.restores.clear();
    for (entity, preview, pose, member, materials, prefab, visibility, children) in &previews {
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
            && !pose.is_changed()
            && !materials.is_changed()
            && !prefab.as_ref().is_some_and(|source| {
                source.is_changed()
                    || changed_scenes.contains(&source.0.id())
                    || changed_prefabs.contains(&source.0.id())
            })
        {
            continue;
        }
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            continue;
        };
        let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &pose)
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
        let inverse_rotation = pose.rotation.inverse();
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
            preview: pose.compute_affine(),
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
        if let Some((translation, size)) =
            footprint_rectangle(footprint, pivot, turns, inverse_rotation)
        {
            project(
                PlacementDecalRole::Footprint,
                translation,
                size,
                footprint_material,
                0.0,
            );
        }
        project(
            PlacementDecalRole::Grid,
            Vec3::new(0.0, 0.015, 0.0),
            Vec2::splat(materials.grid_radius[grid_size] * 2.0),
            grid_material,
            0.0,
        );
        if let Some((arrow, prefab)) = definition.dock_arrow.zip(
            prefab
                .as_ref()
                .and_then(|source| prefab_assets.get(&source.0)),
        ) {
            for index in 0_usize.. {
                let name = if index == 0 {
                    AssetId::from_key("dock_arrow")
                } else {
                    AssetId::from_key(&format!("dock_arrow{index}"))
                };
                let Some(socket) = prefab.prefab_attachment_transform(name) else {
                    break;
                };
                let outward = socket
                    .transform_vector3(Vec3::Z)
                    .with_y(0.0)
                    .normalize_or_zero();
                if outward == Vec3::ZERO {
                    continue;
                }
                let socket = socket.transform_point3(Vec3::ZERO).with_y(0.03);
                let radius = arrow.size_metres;
                let motion = EntranceArrow {
                    socket,
                    outward,
                    radius,
                    phase: 0.0,
                };
                let inward_world = pose.rotation * -outward;
                let role = PlacementDecalRole::EntranceArrow(index);
                let decal = projection.project(
                    &mut commands,
                    &mut meshes,
                    &terrain,
                    &mut decals,
                    existing.remove(&role),
                    role,
                    motion.translation(),
                    Vec2::splat(2.0 * radius),
                    &materials.entrance_arrow,
                    inward_world.x.atan2(inward_world.z),
                );
                if let Ok((.., Some(mut current))) = decals.get_mut(decal) {
                    current.socket = socket;
                    current.outward = outward;
                    current.radius = radius;
                } else {
                    commands.entity(decal).insert(motion);
                }
            }
        }
        for obsolete in existing.into_values() {
            commands.entity(obsolete).try_despawn();
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct PlacementPreviewTint(bool);

pub(super) fn resolve_placement_preview_tint(
    mut previews: Query<
        (
            Entity,
            &ConstructionPreview,
            Option<&mut PlacementPreviewTint>,
        ),
        (
            With<ObjectPlacementPrefabSource>,
            Changed<ConstructionPreview>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, preview, tint) in &mut previews {
        let next =
            PlacementPreviewTint(matches!(preview.validity, PlacementValidity::Valid { .. }));
        if let Some(mut current) = tint {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

pub(super) fn project_object_placement_validity_onto_preview_model_tint(
    previews: Query<(&PlacementPreviewTint, &ObjectPlacementPreviewMaterials)>,
    changed: Query<
        Entity,
        Or<(
            Changed<PlacementPreviewTint>,
            Changed<ObjectPlacementPreviewMaterials>,
        )>,
    >,
    new_nodes: Query<
        Entity,
        Or<(
            Added<ObjectPlacementPreviewRenderable>,
            Added<PrefabModelTint>,
            Added<PlacementDecalRole>,
        )>,
    >,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut renderables: Query<&mut PrefabModelTint, With<ObjectPlacementPreviewRenderable>>,
    mut decals: Query<(&PlacementDecalRole, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let mut dirty: bevy::ecs::entity::EntityHashSet = changed.iter().collect();
    dirty.extend(new_nodes.iter().filter_map(|entity| {
        parents
            .iter_ancestors::<ChildOf>(entity)
            .find(|&ancestor| previews.contains(ancestor))
    }));
    for entity in dirty {
        let Ok((valid, materials)) = previews.get(entity) else {
            continue;
        };
        let (model, footprint) = if valid.0 {
            (materials.model_valid, &materials.footprint_valid)
        } else {
            (materials.model_invalid, &materials.footprint_invalid)
        };
        for descendant in children.iter_descendants_depth_first::<Children>(entity) {
            if let Ok(mut tint) = renderables.get_mut(descendant) {
                tint.map_unchanged(|tint| &mut tint.0).set_if_neq(model);
            }
            if let Ok((PlacementDecalRole::Footprint, mut material)) = decals.get_mut(descendant) {
                material
                    .map_unchanged(|material| &mut material.0)
                    .set_if_neq(footprint.clone());
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
