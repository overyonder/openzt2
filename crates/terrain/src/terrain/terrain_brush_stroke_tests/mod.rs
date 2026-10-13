use std::time::Duration;

use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::terrain::TerrainPresentation;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_game_data::terrain::TerrainSlopeTile;
use openzt2_game_data::terrain::TerrainUnits;
use openzt2_game_data::terrain::TerrainWaterDepth;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;

use crate::terrain::terrain_brush_interaction::update_terrain_brush_preview_and_commit_completed_pointer_strokes;
use crate::terrain::terrain_brush_preview_accumulation::accumulate_terrain_stroke_dabs_into_brush_preview;
use crate::terrain::terrain_edit_application::apply_committed_terrain_sample_deltas_and_publish_changed_regions;
use crate::terrain::terrain_edit_preparation::prepare_terrain_sample_deltas_for_construction_transactions;
use crate::terrain::terrain_live_brush_application::apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::ActiveTerrainBrush;
use openzt2_zoo_state::terrain::terrain_brush_interaction_state::PendingTerrainBrushCommit;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushPreview;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainStroke;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::CommitTerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPreparationRejected;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPrepared;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;

fn stepped_terrain_asset(destination_height_cm: i16) -> TerrainAsset {
    terrain_test_asset(TerrainGrid {
        width: 17,
        height: 17,
        sector_columns: 1,
        sector_rows: 1,
        units: TerrainUnits {
            cell_size_metres: 1.0,
            base_height_metres: 0.0,
        },
        presentation: TerrainPresentation {
            detail_texture: String::new(),
            detail_texture_repetition: 1.0,
            cliff_rise_per_metre: [0.0; 2],
            surface_resolution: 1,
            surface_margin_cells: 0,
            full_alpha_composition: false,
        },
        biomes: Vec::new(),
        samples: (0..17 * 17)
            .map(|index| TerrainSample {
                height_relative_to_base_metres: if index % 17 < 8 {
                    10.0
                } else {
                    f32::from(destination_height_cm) * 0.01
                },
                biome_index: 0,
                has_ground_cover: false,
                water_depth: TerrainWaterDepth::Dry,
                water_surface_flag: false,
                excludes_elevated_path: false,
                support_floor_metres: None,
                height_cap_metres: None,
                height_is_linked: false,
                has_child_height_link: false,
            })
            .collect(),
        water_regions: Vec::new(),
        waterfall: None,
        slope_tiles: vec![
            TerrainSlopeTile {
                uses_alternate_diagonal: false
            };
            16
        ],
    })
}

#[test]
fn flatten_offsets_hold_the_press_height_through_drag_commit_undo_and_redo() {
    for (offset_cm, destination_height_cm) in [
        (0, 600),
        (0, 1400),
        (350, 600),
        (350, 1400),
        (-350, 600),
        (-350, 1400),
    ] {
        let mut app = App::new();
        app.init_resource::<Assets<TerrainAsset>>()
            .init_resource::<Time>()
            .init_resource::<PrimaryPointerInputState>()
            .init_resource::<UiPointerCapture>()
            .insert_resource(ConstructionPlacementPolicy {
                terrain_deformation_brush_radius_half_metre_steps: 2,
                terrain_flatten_strength_tenths_per_second: 0,
                ..default()
            })
            .insert_resource(ConstructionTool::Terrain(TerrainBrushKind::Flatten {
                height_cm: offset_cm,
            }))
            .add_message::<TerrainStroke>()
            .add_message::<CommitConstruction>()
            .add_message::<PrepareConstruction>()
            .add_message::<TerrainChanged>()
            .add_message::<TerrainNavigationChanged>()
            .add_message::<TerrainWaterChanged>()
            .add_message::<TerrainEditPrepared>()
            .add_message::<TerrainEditPreparationRejected>()
            .add_message::<CommitTerrainEdit>()
            .add_message::<TerrainEditAcknowledged>()
            .add_systems(
                Update,
                (
                    update_terrain_brush_preview_and_commit_completed_pointer_strokes,
                    accumulate_terrain_stroke_dabs_into_brush_preview,
                    apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit,
                    prepare_terrain_sample_deltas_for_construction_transactions,
                    apply_committed_terrain_sample_deltas_and_publish_changed_regions,
                )
                    .chain(),
            );
        let asset = app
            .world_mut()
            .resource_mut::<Assets<TerrainAsset>>()
            .add(stepped_terrain_asset(destination_height_cm));
        let chunk = app
            .world_mut()
            .spawn((
                TerrainChunk {
                    source_coord: IVec2::ZERO,
                    coord: IVec2::ZERO,
                    asset,
                    origin: Vec2::ZERO,
                    spacing_m: 1.0,
                    side: 17,
                },
                TerrainChunkId(0),
            ))
            .id();
        app.insert_resource(TerrainIndex {
            chunks: [(IVec2::ZERO, chunk)].into_iter().collect(),
            origin: Vec2::ZERO,
            chunk_span_m: 16.0,
        });
        app.world_mut().spawn(WorldRoot {
            scenario: default(),
        });
        let cursor = app
            .world_mut()
            .spawn(ConstructionCursor {
                // The target comes from canonical ground, not this presentation height.
                world: Vec3::new(4.0, 99.0, 4.0),
                normal: Vec3::Y,
                over_terrain: true,
            })
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.1));
        app.update();
        let preview = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainBrushPreview>>()
            .single(app.world())
            .unwrap();
        *app.world_mut().resource_mut::<PrimaryPointerInputState>() = PrimaryPointerInputState {
            pressed: true,
            just_pressed: true,
            ..default()
        };
        app.world_mut()
            .get_mut::<ConstructionCursor>(cursor)
            .unwrap()
            .over_terrain = false;
        app.update();
        assert!(app.world().get::<ActiveTerrainBrush>(preview).is_none());
        assert!(app.world().get::<EditedTerrainSamples>(chunk).is_none());
        app.world_mut()
            .get_mut::<ConstructionCursor>(cursor)
            .unwrap()
            .over_terrain = true;
        app.world_mut().resource_mut::<UiPointerCapture>().over_ui = true;
        app.update();
        assert!(app.world().get::<ActiveTerrainBrush>(preview).is_none());
        assert!(app.world().get::<EditedTerrainSamples>(chunk).is_none());
        app.world_mut().resource_mut::<UiPointerCapture>().over_ui = false;
        app.update();
        app.world_mut()
            .resource_mut::<PrimaryPointerInputState>()
            .just_pressed = false;
        // A second stationary dab must not add the cliff/ditch offset again.
        app.update();
        app.world_mut()
            .get_mut::<ConstructionCursor>(cursor)
            .unwrap()
            .world
            .x = 12.0;
        app.update();
        let expected = 1000 + offset_cm;
        let edited = app.world().get::<EditedTerrainSamples>(chunk).unwrap();
        assert_eq!(edited.heights_cm[4 * 17 + 12], expected);
        assert_eq!(edited.heights_cm[4 * 17 + 4], expected);
        assert_eq!(edited.heights_cm[0], 1000);
        let committed = edited.clone();
        *app.world_mut().resource_mut::<PrimaryPointerInputState>() = PrimaryPointerInputState {
            just_released: true,
            ..default()
        };
        app.update();
        assert!(app.world().get::<ActiveTerrainBrush>(preview).is_none());
        assert!(app
            .world()
            .get::<PendingTerrainBrushCommit>(preview)
            .is_some());
        assert_eq!(
            app.world().resource::<Messages<CommitConstruction>>().len(),
            1
        );
        let transaction = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(PrepareConstruction {
            transaction,
            preview,
        });
        app.update();
        assert!(app.world().get::<TerrainEdit>(preview).is_none());
        assert!(app.world().get::<TerrainEdit>(transaction).is_some());
        for application in [
            EditApplication::InitialCommit,
            EditApplication::Undo,
            EditApplication::Redo,
        ] {
            app.world_mut().write_message(CommitTerrainEdit {
                transaction,
                application,
            });
            app.update();
            let edited = app.world().get::<EditedTerrainSamples>(chunk).unwrap();
            if application == EditApplication::Undo {
                for (index, height) in edited.heights_cm.iter().enumerate() {
                    assert_eq!(
                        *height,
                        if index % 17 < 8 {
                            1000
                        } else {
                            destination_height_cm
                        }
                    );
                }
            } else {
                assert_eq!(edited, &committed);
            }
        }
        // A fresh press samples the edited surface rather than reusing the last target.
        app.world_mut().entity_mut(preview).despawn();
        *app.world_mut().resource_mut::<PrimaryPointerInputState>() = default();
        app.update();
        *app.world_mut().resource_mut::<PrimaryPointerInputState>() = PrimaryPointerInputState {
            pressed: true,
            just_pressed: true,
            ..default()
        };
        app.update();
        assert_eq!(
            app.world()
                .get::<EditedTerrainSamples>(chunk)
                .unwrap()
                .heights_cm[4 * 17 + 12],
            expected + offset_cm
        );
    }
}
