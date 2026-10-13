use super::verification_journey_script_parsing::{
    read_verification_journey_file, verification_navigation_routes,
};

fn journey_directory() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/verification/journeys")
}

#[test]
fn every_journey_and_navigation_route_parses() {
    verification_navigation_routes().unwrap();
    let mut journeys = 0;
    for entry in std::fs::read_dir(journey_directory()).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "journey")
        {
            read_verification_journey_file(&path).unwrap();
            journeys += 1;
        }
    }
    assert!(journeys > 0);
}
