use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
use openzt2_game_data::world_definitions::animal_health::{
    DiseaseAnimalFilter, DiseaseComparison, DiseaseZooTest,
};
use openzt2_game_data::AssetId;

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

#[test]
fn disease_manager_keeps_each_disease_and_its_exposure_rules() {
    let manager = parse_blue_fang_source_document(
        AssetPath::new("config/diseasemgr.xml"),
        br#"<ZTDiseaseMgr halfStarMinimum="1" diseaseCheckInterval="720" updateInterval="3" cureTimeout="2880">
  <diseases>
    <ZTDisease name="PoopingDisease" token="t_GetPoopingDisease" cureToken="t_CurePoopingDisease" clearTokens="t_GetPoopingDisease t_SpreadPoopingDisease" targetTypes="animal" chance="10" singleQualifier="true">
      <qualifier>
        <zooTests>
          <ZTEntityTypePredicate type="Poop" quantity="5" test="GE" different="true" />
          <ZTZooFamePredicate fameHalfStars="4" test="GE" />
        </zooTests>
        <animalFilters>
          <ZTAnimalAttributeFilter attribute="b_ExtinctAnimal" boolValue="true" />
          <ZTAnimalProximityFilter type="TrashGround" distance="10" closer="true" />
        </animalFilters>
      </qualifier>
      <cause types="Poop" hint="exact" />
    </ZTDisease>
    <ZTDisease name="ExtinctFlu" token="t_Disease" targetTypes="animal" chance="1" chanceMultiplier="TrashGround" />
    <ZTDisease name="CatScratchFever" token="t_GetCatScratchDisease" targetTypes="Felidae" chance="10" searchForTargets="false" />
    <ZTDisease name="TutorialLove" token="t_GetTutorialLoveDisease" targetTypes="animal" chance="0" forceOnly="true" />
  </diseases>
</ZTDiseaseMgr>"#,
    )
    .unwrap();
    let document = lower_resolved_world_definition_source_document_closure_to_canonical_document(
        std::slice::from_ref(&manager),
        &BlueFangActorManifestModelAndSceneResolutionIndex::default(),
        "config/diseasemgr.xml",
        [0, 0],
    )
    .unwrap()
    .unwrap();
    let policy = document.disease_exposure.unwrap();
    assert_eq!(policy.check_interval_seconds, 720.0);
    assert_eq!(policy.cure_timeout_seconds, 2880.0);
    let [pooping, flu, scratch, love] = &policy.diseases[..] else {
        panic!("expected four diseases, found {:?}", policy.diseases);
    };
    let key = |name: &str| AssetId::from_key(&name.to_ascii_lowercase());
    assert_eq!(pooping.name, key("PoopingDisease"));
    assert_eq!(pooping.token, key("t_GetPoopingDisease"));
    assert_eq!(
        pooping.clear_tokens,
        [key("t_GetPoopingDisease"), key("t_SpreadPoopingDisease")]
    );
    assert_eq!(pooping.target_types, [key("animal")]);
    assert_eq!(pooping.chance_percent, 10.0);
    assert!(pooping.single_qualifier && pooping.search_for_targets && !pooping.force_only);
    assert_eq!(
        pooping.zoo_tests,
        [
            DiseaseZooTest::EntityCount {
                entity_type: key("Poop"),
                attribute: None,
                quantity: 5,
                comparison: DiseaseComparison::GreaterOrEqual,
                different: true,
            },
            DiseaseZooTest::Fame {
                half_stars: 4,
                comparison: DiseaseComparison::GreaterOrEqual,
            },
        ]
    );
    assert_eq!(
        pooping.animal_filters,
        [
            DiseaseAnimalFilter::Attribute {
                attribute: key("b_ExtinctAnimal"),
                value: true,
            },
            DiseaseAnimalFilter::Proximity {
                entity_type: key("TrashGround"),
                distance_m: 10.0,
                closer: true,
            },
        ]
    );
    assert_eq!(flu.chance_multiplier, Some(key("TrashGround")));
    assert!(!scratch.search_for_targets);
    assert!(love.force_only);
}
