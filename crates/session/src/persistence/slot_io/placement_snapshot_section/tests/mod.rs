use super::*;
use crate::persistence::slot_io::donation_snapshot_section::{
    apply_donation_records, capture_donation_records, decode_donation_records,
    encode_donation_records, resolve_donation_records, DonationSnapshotEntities,
};
use crate::persistence::slot_io::{
    economy_snapshot_application::apply_economy_snapshot_records_to_live_economy_resources_and_facilities,
    economy_snapshot_section_encoding::{
        append_encoded_economy_snapshot_section, decode_and_validate_economy_snapshot_section,
    },
    economy_snapshot_types::{EconomySnapshotRecords, FacilityEconomySnapshotRecord},
};
use bevy::ecs::system::SystemState;
use openzt2_construction::placement::persistence_test_support::{
    add_placement_persistence_test_systems, apply_settled_placement_purchase_for_test,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    scene_prefab::{PrefabEntity, PrefabTransform, ScenePrefabDocument, ScenePrefabEntityFlags},
    world_definitions::{
        document::WorldDefinitionDocument,
        object_placement::{FootprintCell, FootprintCellFlags, PlaceableDefinition},
        world_objects::WorldObjectDefinition,
    },
};
use openzt2_visitors::donations::{
    add_donation_payment_test_systems, queue_arrived_box_donation_for_test,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::economy::{
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    monthly_finance_types::MonthlyFinanceHistory,
};
use openzt2_zoo_state::{
    donations::{
        donation_opportunity_types::{DonationOpportunity, DonationUsePoint},
        donation_payment_types::{DonationCategoryTotals, DonationTotal},
    },
    economy::{
        account_transaction_types::{Account, TransactionKind, TransactionRequest},
        facility_economy_types::{FacilityProfit, OperatingSinceDay, Wallet},
        money_types::Money,
        zoo_cash_types::ZooCash,
    },
    guests::guest_simulation_types::Guest,
    placement::{
        placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
        placement_transaction_types::{
            PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation,
        },
    },
};

fn document() -> WorldDefinitionDocument {
    let id = AssetId::from_key("donationbox-test");
    WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            audio_cue_prefixes: Vec::new(),
            surface_effect_strength: None,
            presentation_caustic_policies: Vec::new(),
            id,
            kind: WorldObjectKind::DonationBox,
            supports_show_tricks: false,
            behavior_type_identifiers: vec![],
            view_data: vec![],
            detach_actions: vec![],
            selected_ui_broadcasts: vec![],
            information_panel: id,
            view_class: None,
            biome_automatic_placement_class: None,
            name_key: id,
            description_key: id,
            zoopedia_subject: id,
            prefab: id,
            catalogue_preview_prefab: None,
            presentation_attachments: vec![],
            named_physical_presentations: vec![],
            interaction_slots: vec![],
            container_quantity: None,
            transactions: vec![],
            prefab_scale: 1.0,
            icon: id,
            biomes: vec![],
            location: id,
            preview_offset_cm: [0; 3],
            preview_scale: 1.0,
            terrain_fitted: false,
            real_physics_water_impact: None,
            price_cents: 1250,
            upkeep_cents_per_month: 0,
            properties: WorldObjectPropertyFlags::DONATION_ACCEPTOR,
            affordances: Default::default(),
            destruction: None,
        }],
        placeables: vec![PlaceableDefinition {
            id,
            weight: 1.0,
            footprint: vec![FootprintCell {
                offset: [0, 0],
                flags: FootprintCellFlags::OCCUPIED,
            }],
            pivot_cm: [0; 2],
            diagonal_footprint: vec![],
            diagonal_pivot_cm: [0; 2],
            rotation_increment_degrees: 90,
            constraints: Default::default(),
            max_slope_permille: 250,
            minimum_headroom_metres: 2.0,
            apply_height_modifier: true,
            price_cents: 1250,
            unlock: id,
            entrances: vec![],
            moving_footprint: false,
            automatic_footprint: false,
            ground_paths_block_placement: false,
        }],
        ..Default::default()
    }
}

fn fixture() -> (App, Entity, Handle<ScenePrefabAsset>) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
        .init_resource::<Assets<ScenePrefabAsset>>()
        .init_resource::<Assets<Gltf>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<AdmissionPrice>()
        .init_resource::<ZooAdmissionsOpen>()
        .init_resource::<MonthlyFinanceHistory>()
        .insert_resource(ZooClock {
            tick: 1,
            absolute_day: 3,
            tick_in_day: 1,
        })
        .configure_sets(
            Update,
            (
                FixedGameSet::Think,
                FixedGameSet::Economy,
                FixedGameSet::Cleanup,
            )
                .chain(),
        );
    add_placement_persistence_test_systems(&mut app);
    openzt2_economy::economy::add_account_transaction_test_systems(&mut app);
    add_donation_payment_test_systems(&mut app);
    app.world_mut().resource_mut::<ZooCash>().0 = Money(10_000);
    let document = document();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let root = app.world_mut().spawn_empty().id();
    app.insert_resource(PersistentIdAllocator::new(root));
    let entity = |attachment, translation, children| PrefabEntity {
        stable_id: AssetId::from_key(attachment),
        attachment_id: AssetId::from_key(attachment),
        model_joint_binding: None,
        local_render_bound: None,
        skin_render_bound: None,
        parent: 0,
        transform: PrefabTransform {
            translation_m: translation,
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        children,
        flags: ScenePrefabEntityFlags(
            ScenePrefabEntityFlags::VISIBLE.0 | ScenePrefabEntityFlags::ACTIVE.0,
        ),
        rotation_cycles: vec![],
        transform_animations: vec![],
        renderables: vec![],
        colliders: vec![],
        lods: vec![],
        billboards: vec![],
        lights: vec![],
        effects: vec![],
    };
    let prefab = app
        .world_mut()
        .resource_mut::<Assets<ScenePrefabAsset>>()
        .add(ScenePrefabAsset::from_test_document(ScenePrefabDocument {
            entities: vec![
                entity("root", [0.0, 0.5, 0.0], vec![1]),
                entity("adultdock", [0.0, 0.0, 1.0], vec![]),
            ],
            automatic_placement_bounds_xz: None,
            rail_camera: None,
            dependencies: vec![],
        }));
    (app, root, prefab)
}

fn section(bytes: &[u8], count: u32) -> WorldSnapshotSectionDirectoryEntry {
    WorldSnapshotSectionDirectoryEntry {
        encoded_record_count: count,
        payload_start_byte_offset: 0,
        payload_byte_count: bytes.len() as u64,
    }
}

fn capture(app: &mut App) -> Vec<PlacementSnapshotRecord> {
    let mut state = SystemState::<(
        PlacementSnapshotEntities,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
    )>::new(app.world_mut());
    let (entities, active, assets) = state.get(app.world()).unwrap();
    capture_placement_records(&entities, active.get(&assets).unwrap()).unwrap()
}

fn donate(app: &mut App, root: Entity, acceptor: Entity) {
    assert!(app.world().get::<DonationUsePoint>(acceptor).is_some());
    let guest = app
        .world_mut()
        .spawn((Guest, Wallet(Money(1000)), WorldMember { root }))
        .id();
    let subject = app
        .world_mut()
        .spawn((
            WorldMember { root },
            DonationOpportunity {
                category: AssetId::from_key("Donate_Lion"),
                amount: Money(500),
                interaction_ticks: 0,
            },
        ))
        .id();
    queue_arrived_box_donation_for_test(app.world_mut(), guest, acceptor, subject, Money(500));
    app.update();
    assert_eq!(app.world().get::<Wallet>(guest).unwrap().0, Money(500));
}

#[test]
fn purchased_box_survives_cold_restore_and_accepts_another_completed_payment() {
    let (mut source, root, prefab) = fixture();
    let purchase = source.world_mut().spawn_empty().id();
    source.world_mut().write_message(TransactionRequest {
        operation: purchase,
        debit: Account::Zoo,
        credit: Account::External,
        amount: Money(1250),
        kind: TransactionKind::Construction,
        subject: None,
    });
    source.update();
    assert_eq!(source.world().resource::<ZooCash>().0, Money(8750));
    let acceptor = apply_settled_placement_purchase_for_test(
        source.world_mut(),
        root,
        purchase,
        PreparedObjectPlacementMutation {
            entity: PersistentId(42),
            definition: AssetId::from_key("donationbox-test"),
            transform: Transform::from_xyz(4.0, 0.0, -7.0),
            cells: vec![IVec2::new(4, -7)].into_boxed_slice(),
            previous_transform: None,
            previous_cells: Box::new([]),
            mutation: PreparedObjectPlacementEditMutationKind::Create,
            applied_entity: None,
            preview: None,
            prefab,
        },
    );
    source.update();
    source.update();
    donate(&mut source, root, acceptor);
    let expected = capture(&mut source);
    let bytes = encode_placement_records(&expected).unwrap();
    let records = decode_placement_records(&bytes, section(&bytes, 1))
        .unwrap()
        .unwrap();
    let mut donation_capture = SystemState::<DonationSnapshotEntities>::new(source.world_mut());
    let donations =
        capture_donation_records(&donation_capture.get(source.world()).unwrap()).unwrap();
    let donation_bytes = encode_donation_records(&donations).unwrap();
    let donations = decode_donation_records(&donation_bytes, section(&donation_bytes, 1)).unwrap();
    let saved_cash = source.world().resource::<ZooCash>().0;
    let saved_profit = *source.world().get::<FacilityProfit>(acceptor).unwrap();
    let saved_age = *source.world().get::<OperatingSinceDay>(acceptor).unwrap();
    let economy = EconomySnapshotRecords {
        zoo_cash: saved_cash,
        admission_price: Money(250),
        admissions_open: true,
        current_calendar_month: 0,
        lifetime_income: Money(500),
        lifetime_expenses: Money(1250),
        lifetime_donation_income: Money(500),
        lifetime_users: 0,
        monthly_finance_history_initialized: false,
        monthly_finance_records: vec![],
        facility_economy_records: vec![FacilityEconomySnapshotRecord {
            persistent_identifier: PersistentId(42),
            opening_zoo_day: saved_age.0,
            total_profit_cents: saved_profit.total.0,
            transaction_count: saved_profit.transactions,
        }],
    };
    let mut economy_bytes = Vec::new();
    append_encoded_economy_snapshot_section(&mut economy_bytes, &economy);
    let economy =
        decode_and_validate_economy_snapshot_section(&economy_bytes, section(&economy_bytes, 2))
            .unwrap();

    let (mut destination, restored_root, restored_prefab) = fixture();
    // This baseline has no purchased box. A different deleted baseline identity must disappear.
    let deleted = destination
        .world_mut()
        .spawn((
            PersistentId(5),
            DefinitionId(AssetId::from_key("donationbox-test")),
            Transform::from_xyz(20.0, 0.0, 20.0),
            WorldMember {
                root: restored_root,
            },
        ))
        .id();
    let mut state = SystemState::<(
        Commands,
        Res<Assets<ScenePrefabAsset>>,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
        ResMut<PersistentIdAllocator>,
        ResMut<ZooCash>,
        ResMut<AdmissionPrice>,
        ResMut<ZooAdmissionsOpen>,
        ResMut<MonthlyFinanceHistory>,
    )>::new(destination.world_mut());
    let restored_acceptor = {
        let (
            mut commands,
            prefabs,
            active,
            assets,
            mut allocator,
            mut cash,
            mut price,
            mut admissions,
            mut history,
        ) = state.get_mut(destination.world_mut()).unwrap();
        let restored = apply_placement_records(
            &mut commands,
            &records,
            &prefabs,
            &[restored_prefab],
            active.get(&assets).unwrap(),
            &[deleted],
            restored_root,
            &mut allocator,
        )
        .unwrap();
        let restored_acceptor = restored[&42];
        apply_donation_records(
            &mut commands,
            resolve_donation_records(donations, &restored).unwrap(),
        );
        apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
            &mut commands,
            economy,
            &restored,
            &mut cash,
            &mut price,
            &mut admissions,
            &mut history,
        )
        .unwrap();
        restored_acceptor
    };
    state.apply(destination.world_mut());
    destination.update();
    destination.update();
    assert!(destination.world().get_entity(deleted).is_err());
    assert_eq!(capture(&mut destination), expected);
    assert_eq!(destination.world().resource::<ZooCash>().0, saved_cash);
    assert_eq!(
        destination.world().resource::<AdmissionPrice>().0,
        Money(250)
    );
    assert!(destination.world().resource::<ZooAdmissionsOpen>().0);
    assert_eq!(
        destination
            .world()
            .get::<OperatingSinceDay>(restored_acceptor)
            .unwrap()
            .0,
        saved_age.0
    );
    assert_eq!(
        destination
            .world()
            .get::<FacilityProfit>(restored_acceptor)
            .unwrap()
            .transactions,
        saved_profit.transactions
    );
    assert!(destination
        .world()
        .resource::<PlacedObjectFootprintOccupancyIndex>()
        .cell_contains_entity(IVec2::new(4, -7), restored_acceptor));
    assert_eq!(
        destination
            .world()
            .get::<DonationTotal>(restored_acceptor)
            .unwrap()
            .amount,
        Money(500)
    );
    donate(&mut destination, restored_root, restored_acceptor);
    assert_eq!(
        destination
            .world()
            .get::<DonationCategoryTotals>(restored_acceptor)
            .unwrap()
            .aggregate_for_category(Some(AssetId::from_key("Donate_Lion")))
            .unwrap()
            .amount,
        Money(1000)
    );
    assert_eq!(
        destination
            .world()
            .get::<FacilityProfit>(restored_acceptor)
            .unwrap()
            .total,
        Money(1000)
    );
    assert_eq!(
        destination.world().resource::<ZooCash>().0,
        Money(saved_cash.0 + 500)
    );
}

#[test]
fn restored_tilted_world_pose_preserves_transform_and_completes_non_grid_hydration() {
    let (mut app, root, prefab) = fixture();
    let expected = vec![PlacementSnapshotRecord {
        persistent_identifier: 42,
        definition: AssetId::from_key("donationbox-test"),
        transform: Transform {
            translation: Vec3::new(4.5, 2.0, -6.5),
            rotation: Quat::from_euler(EulerRot::YXZ, 0.3, 0.02, 0.01),
            scale: Vec3::splat(0.5),
        },
    }];
    let bytes = encode_placement_records(&expected).unwrap();
    let records = decode_placement_records(&bytes, section(&bytes, 1))
        .unwrap()
        .unwrap();
    let mut state = SystemState::<(
        Commands,
        Res<Assets<ScenePrefabAsset>>,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
        ResMut<PersistentIdAllocator>,
    )>::new(app.world_mut());
    let restored = {
        let (mut commands, prefabs, active, assets, mut allocator) =
            state.get_mut(app.world_mut()).unwrap();
        apply_placement_records(
            &mut commands,
            &records,
            &prefabs,
            &[prefab],
            active.get(&assets).unwrap(),
            &[],
            root,
            &mut allocator,
        )
        .unwrap()[&42]
    };
    state.apply(app.world_mut());
    app.update();
    app.update();
    assert_eq!(capture(&mut app), expected);
    assert!(app
        .world()
        .get::<PlacedObjectDefinitionReference>(restored)
        .is_none());
    assert!(app.world().get::<DonationUsePoint>(restored).is_some());
    let definitions = app
        .world()
        .resource::<WorldDefinitions>()
        .get(app.world().resource::<Assets<WorldDefinitionAsset>>())
        .unwrap();
    assert!(placement_hydration_complete(
        definitions,
        expected[0].definition,
        &expected[0].transform,
        false,
        true
    ));
    assert!(!placement_hydration_complete(
        definitions,
        expected[0].definition,
        &expected[0].transform,
        false,
        false
    ));
    assert!(!placement_hydration_complete(
        definitions,
        expected[0].definition,
        &Transform::IDENTITY,
        false,
        true
    ));
}

#[test]
fn missing_placement_prefab_leaves_baseline_population_intact() {
    let (mut app, root, prefab) = fixture();
    app.world_mut()
        .resource_mut::<Assets<ScenePrefabAsset>>()
        .remove(prefab.id());
    let baseline = app.world_mut().spawn_empty().id();
    let records = [PlacementSnapshotRecord {
        persistent_identifier: 42,
        definition: AssetId::from_key("donationbox-test"),
        transform: Transform::IDENTITY,
    }];
    let mut state = SystemState::<(
        Commands,
        Res<Assets<ScenePrefabAsset>>,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
        ResMut<PersistentIdAllocator>,
    )>::new(app.world_mut());
    {
        let (mut commands, prefabs, active, assets, mut allocator) =
            state.get_mut(app.world_mut()).unwrap();
        assert!(apply_placement_records(
            &mut commands,
            &records,
            &prefabs,
            &[prefab],
            active.get(&assets).unwrap(),
            &[baseline],
            root,
            &mut allocator
        )
        .is_err());
    }
    state.apply(app.world_mut());
    assert!(app.world().get_entity(baseline).is_ok());
}

#[test]
fn legacy_empty_section_and_saved_empty_population_have_distinct_meanings() {
    assert_eq!(
        decode_placement_records(&[], section(&[], 0)).unwrap(),
        None
    );
    let bytes = encode_placement_records(&[]).unwrap();
    assert_eq!(
        decode_placement_records(&bytes, section(&bytes, 0)).unwrap(),
        Some(vec![])
    );
}
