use super::{
    super::{
        persistence_failure_types::WorldSnapshotPersistenceFailure,
        snapshot_container_encoding_and_validation::{
            append_encoded_world_snapshot_section_to_container,
            finalize_world_snapshot_container_header_and_checksum,
            initialize_empty_world_snapshot_container,
            validate_world_snapshot_container_and_section_directory,
        },
        snapshot_container_types::{WorldSnapshotSectionDirectoryEntry, WorldSnapshotSectionKind},
    },
    animal_snapshot_application::apply_animal_snapshot_records,
    animal_snapshot_capture::{capture_animal_snapshot_records, AnimalSnapshotEntities},
    animal_snapshot_section_encoding::{
        decode_animal_snapshot_records, encode_animal_snapshot_records,
    },
    animal_snapshot_types::AnimalSnapshotRecord,
};
use crate::plugins::{
    animal_lifecycle::{animal_spawn_resolution_types::ResolvedAnimalSpawnComponents, types::*},
    animal_welfare::types::*,
    habitat::habitat_types::{HabitatLocatable, HabitatMember},
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use bevy::{ecs::system::SystemState, prelude::*};
use openzt2_game_data::{
    species::{LifeStage, Sex, SpeciesFlags, SpeciesVariantFlags},
    AssetId,
};

fn spawn_named_animal(
    world: &mut World,
    id: u64,
    name: &str,
    sex: Sex,
    stage: LifeStage,
) -> Entity {
    let animal = world
        .spawn((
            Animal,
            PersistentId(id),
            SpeciesHandle {
                species: AssetId([1; 16]),
            },
            AnimalVariant(AssetId([2; 16])),
            AnimalSex(sex),
            AnimalLifeStage(stage),
            Age { born_tick: 81 },
            Parents::NONE,
            Name::new(name.to_owned()),
            Transform::from_xyz(12.75 + id as f32, 3.5, -42.125)
                .with_rotation(Quat::from_rotation_y(0.75))
                .with_scale(Vec3::new(1.1, 1.2, 1.3)),
        ))
        .id();
    world.entity_mut(animal).insert((
        Hunger(11 * Q16_ONE + 7),
        Thirst(22 * Q16_ONE + 5),
        RestNeed(33 * Q16_ONE),
        PrivacyNeed(44 * Q16_ONE),
        SocialNeed(55 * Q16_ONE),
        ExerciseNeed(66 * Q16_ONE),
        StimulationNeed(77 * Q16_ONE),
        EnvironmentNeed(88 * Q16_ONE),
        HealthNeed(99 * Q16_ONE),
        HygieneNeed(111 * Q16_ONE),
        BathroomNeed(222 * Q16_ONE),
    ));
    world.entity_mut(animal).insert((
        AnimalWelfare(72),
        WelfareBand::Poor,
        NeedTriggerState(0x505),
        CumulativeNeedPoints {
            good: 123.5,
            bad: 71.25,
        },
        HabitatSuitability {
            space: 600,
            biome: 700,
            overall: 650,
        },
    ));
    animal
}

fn capture(world: &mut World) -> Vec<AnimalSnapshotRecord> {
    let mut state = SystemState::<AnimalSnapshotEntities>::new(world);
    capture_animal_snapshot_records(&state.get(world).unwrap()).unwrap()
}

fn resolved_components(record: &AnimalSnapshotRecord) -> ResolvedAnimalSpawnComponents {
    ResolvedAnimalSpawnComponents {
        sex: record.sex,
        life_stage: record.life_stage,
        variant: record.variant,
        definition: AssetId([3; 16]),
        life_stage_start_ticks: AuthoredAnimalLifeStageStartTicks {
            stage_start_ticks: [0, 10, 20, 100],
        },
        reproduction: ReproductionTraits {
            gestation_ticks: 90,
            litter: [1, 2],
        },
        waste: AnimalWasteCycle {
            definition: AssetId([4; 16]),
            interval_ticks: 30,
            units: 1,
        },
        presentation_model: SelectedAnimalPresentationModel {
            model: AssetId([5; 16]),
        },
        scale: 1.0,
        species_flags: SpeciesFlags::default(),
        variant_flags: SpeciesVariantFlags::default(),
    }
}

#[test]
fn populated_animal_ecs_survives_container_round_trip_into_a_different_world() {
    let mut source = World::new();
    // The child's ID sorts before both parents, requiring a second reference pass.
    let mother = spawn_named_animal(&mut source, 51, "Mère", Sex::Female, LifeStage::Adult);
    let father = spawn_named_animal(&mut source, 72, "Father", Sex::Male, LifeStage::Elder);
    let child = spawn_named_animal(&mut source, 9, "Cub", Sex::Female, LifeStage::Young);
    source.entity_mut(child).insert(Parents {
        mother: Some(mother),
        father: Some(father),
    });
    source.entity_mut(mother).insert(Pregnancy {
        father,
        due_tick: 999,
    });
    source.entity_mut(mother).insert((
        ReleaseRestricted,
        CratingRestricted,
        crate::plugins::world_spawn::world_entity_crating::WorldEntityIsCrated,
    ));
    source
        .entity_mut(father)
        .insert(crate::plugins::animal_health::types::Dead {
            cause: crate::plugins::animal_health::types::DeathCause::Disease(AssetId([8; 16])),
            since_tick: 200,
        });
    let expected = capture(&mut source);
    let profile = AssetId([7; 16]);
    let payload = encode_animal_snapshot_records(&expected).unwrap();
    let mut container = Vec::new();
    initialize_empty_world_snapshot_container(&mut container, profile);
    for kind in WorldSnapshotSectionKind::ENCODING_ORDER {
        append_encoded_world_snapshot_section_to_container(
            &mut container,
            kind,
            if kind == WorldSnapshotSectionKind::Animals {
                3
            } else {
                0
            },
            |bytes| {
                if kind == WorldSnapshotSectionKind::Animals {
                    bytes.extend_from_slice(&payload);
                }
            },
        )
        .unwrap();
    }
    finalize_world_snapshot_container_header_and_checksum(&mut container, 3).unwrap();
    let validated =
        validate_world_snapshot_container_and_section_directory(&container, profile).unwrap();
    let records = decode_animal_snapshot_records(
        &container,
        validated.section_directory_entry(WorldSnapshotSectionKind::Animals),
    )
    .unwrap();

    let mut destination = World::new();
    for _ in 0..12 {
        destination.spawn_empty();
    }
    let root = destination.spawn_empty().id();
    let unrelated = destination.spawn((PersistentId(4), Name::new("keep"))).id();
    let baseline = destination.spawn((Animal, PersistentId(51))).id();
    destination.insert_resource(PersistentIdAllocator::new(root));
    let resolved = records.iter().map(resolved_components).collect();
    let mut state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        Query<Entity, With<Animal>>,
    )>::new(&mut destination);
    let restored = {
        let (mut commands, mut allocator, baseline_animals) =
            state.get_mut(&mut destination).unwrap();
        apply_animal_snapshot_records(
            &mut commands,
            records,
            resolved,
            root,
            &mut allocator,
            &baseline_animals,
        )
        .unwrap()
    };
    state.apply(&mut destination);
    assert!(destination.get_entity(baseline).is_err());
    assert!(destination.get_entity(unrelated).is_ok());
    assert_eq!(capture(&mut destination), expected);
    assert_ne!(restored[&9], child);
    let restored_child = restored[&9];
    assert_eq!(
        destination.get::<Parents>(restored_child).unwrap().mother,
        Some(restored[&51])
    );
    assert_eq!(
        destination.get::<Parents>(restored_child).unwrap().father,
        Some(restored[&72])
    );
    assert_eq!(
        destination.get::<Pregnancy>(restored[&51]).unwrap().father,
        restored[&72]
    );
    assert_eq!(
        destination.get::<Name>(restored_child).unwrap().as_str(),
        "Cub"
    );
    assert_eq!(
        destination.get::<Hunger>(restored_child).unwrap().0,
        11 * Q16_ONE + 7
    );
    assert_eq!(
        destination.get::<WorldMember>(restored_child).unwrap().root,
        root
    );
    assert_eq!(
        destination.get::<DefinitionId>(restored_child).unwrap().0,
        AssetId([3; 16])
    );
    assert!(destination
        .get::<SelectedAnimalPresentationModel>(restored_child)
        .is_some());
    assert!(destination
        .get::<HabitatLocatable>(restored_child)
        .is_some());
    assert!(destination.get::<HabitatMember>(restored_child).is_none());
    // The initializer selects Without<AnimalWelfare>; loaded needs must not be rerolled.
    assert_eq!(
        destination
            .query_filtered::<Entity, (With<Animal>, Without<AnimalWelfare>)>()
            .iter(&destination)
            .count(),
        0
    );
    assert_eq!(
        destination
            .resource_mut::<PersistentIdAllocator>()
            .allocate(root)
            .unwrap(),
        PersistentId(73)
    );

    // Version 11's Animals section meant adoption offers and must not be reinterpreted.
    container[8..12].copy_from_slice(&11_u32.to_le_bytes());
    assert_eq!(
        validate_world_snapshot_container_and_section_directory(&container, profile),
        Err(WorldSnapshotPersistenceFailure::UnsupportedSnapshotFormatVersion)
    );
}

#[test]
fn empty_saved_population_removes_baseline_animals() {
    let mut destination = World::new();
    let root = destination.spawn_empty().id();
    let baseline = destination.spawn((Animal, PersistentId(9))).id();
    destination.insert_resource(PersistentIdAllocator::new(root));
    let bytes = encode_animal_snapshot_records(&[]).unwrap();
    let records = decode_animal_snapshot_records(
        &bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: 0,
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
    .unwrap();
    let mut state = SystemState::<(
        Commands,
        ResMut<PersistentIdAllocator>,
        Query<Entity, With<Animal>>,
    )>::new(&mut destination);
    {
        let (mut commands, mut allocator, animals) = state.get_mut(&mut destination).unwrap();
        assert!(apply_animal_snapshot_records(
            &mut commands,
            records,
            vec![],
            root,
            &mut allocator,
            &animals
        )
        .unwrap()
        .is_empty());
    }
    state.apply(&mut destination);
    assert!(destination.get_entity(baseline).is_err());
    assert!(capture(&mut destination).is_empty());
}
