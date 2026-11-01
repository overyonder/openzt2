use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn environment_retains_distinct_light_and_fog_programs() {
    let environment = parse_blue_fang_source_document(
        AssetPath::new("world/environments/env1coastal_small.xml"),
        br#"<BFWorldEnvironment lights="world/lights/ScrubLights.xml"><fog file="world/environments/coastal_fog.xml"/></BFWorldEnvironment>"#,
    ).unwrap();
    let document = lower_resolved_world_definition_source_document_closure_to_canonical_document(
        &[environment],
        &BlueFangActorManifestModelAndSceneResolutionIndex::default(),
        "world/environments/env1coastal_small.xml",
        [0, 0],
    )
    .unwrap()
    .unwrap();
    let environment = &document.environments[0];
    assert_eq!(
        environment.family,
        openzt2_game_data::AssetId::from_key("coastal")
    );
    assert_eq!(
        environment.light_program,
        Some(openzt2_game_data::AssetId::from_key("scrub"))
    );
    assert!(environment.light_samples.is_empty());
}

#[test]
fn fossil_policy_requires_mode_dependency_and_preserves_its_authored_dig_range() {
    let manager = parse_blue_fang_source_document(
        AssetPath::new("config/puzzlemgr.xml"),
        br#"<ZTPuzzleMgr puzzleRoot="puzzles" entityRoot="entities/objects/puzzles" placeableObjects="toy" nonPlaceableObjects="gift"/>"#,
    ).unwrap();
    let index = BlueFangActorManifestModelAndSceneResolutionIndex::default();
    assert!(
        lower_resolved_world_definition_source_document_closure_to_canonical_document(
            std::slice::from_ref(&manager),
            &index,
            "config/puzzlemgr.xml",
            [0, 0],
        )
        .is_err(),
        "missing mode policy must not silently publish an empty manager"
    );
    for distance in [1.75, 3.5] {
        let source = format!(
            r#"<Modes><ZTFossilFindingMode minDistance="6" maxDistance="24" sonarViewCone="90" digDistance="{distance}"/></Modes>"#
        );
        let mode = parse_blue_fang_source_document(
            AssetPath::new("ui/modes/modes.xml"),
            source.as_bytes(),
        )
        .unwrap();
        let document =
            lower_resolved_world_definition_source_document_closure_to_canonical_document(
                &[manager.clone(), mode],
                &index,
                "config/puzzlemgr.xml",
                [0, 0],
            )
            .unwrap()
            .unwrap();
        let policy = document.fossil_placement.unwrap();
        assert_eq!(policy.dig_distance_m, distance);
        assert_eq!(policy.minimum_sonar_distance_squared, 36.0);
        assert_eq!(policy.maximum_sonar_distance_squared, 576.0);
        assert!((policy.minimum_sonar_view_dot - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }
}
