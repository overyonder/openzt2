//! Recover declaring file paths for older snapshot descriptors.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};

use openzt2_game_data::{
    AssetId,
    world_scenario::{ScenarioDefinitionRecord, ScenarioScriptBinding},
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

use super::{
    authored_world_scenario_state::AuthoredWorldScenarioState,
    world_scenario_document_source::WorldScenarioDocumentSource,
};

struct LegacyScenarioDeclaration {
    path: PathBuf,
    scenario: ScenarioDefinitionRecord,
    bindings: Vec<ScenarioScriptBinding>,
    authored: AuthoredWorldScenarioState,
}

pub(super) fn resolve_legacy_world_script_paths(
    source: &mut WorldScenarioDocumentSource,
    archives: &AssetArchives,
) -> io::Result<()> {
    if !source.authored.legacy_script_references {
        return Ok(());
    }

    let mut scenario_origins = BTreeMap::<AssetId, Vec<LegacyScenarioDeclaration>>::new();
    if !source.document.scenarios.is_empty() {
        let mut saved_scenarios = BTreeMap::new();
        for scenario in &source.document.scenarios {
            saved_scenarios.insert(scenario.id, scenario.id);
            if let Some(rule) = scenario.objectives.first() {
                saved_scenarios.insert(rule.id, scenario.id);
            }
            if let Some(group) = scenario.objective_groups.first() {
                saved_scenarios.insert(group.id, scenario.id);
            }
        }
        let goal_paths = archives.source_paths_under(Path::new("scenario/goals"));
        let paths = archives.resolved_paths();
        for path in paths.iter().chain(&goal_paths) {
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let keys = [
                AssetId::from_key(stem),
                AssetId::from_key(&format!("{stem}:rule:0")),
                AssetId::from_key(&format!("{stem}:group:0")),
            ];
            if let Some(&saved_id) = keys.iter().find_map(|key| saved_scenarios.get(key)) {
                let origins = scenario_origins.entry(saved_id).or_default();
                if !origins.iter().any(|origin| origin.path == *path) {
                    if let Some(declaration) = read_scenario_declaration(archives, path) {
                        origins.push(declaration);
                    }
                }
            }
        }
    }

    for scenario in &mut source.document.scenarios {
        let Some(origins) = scenario_origins.get(&scenario.id) else {
            continue;
        };
        let mut matching = origins.iter().filter(|origin| {
            origin
                .scenario
                .objectives
                .iter()
                .map(|rule| rule.id)
                .eq(scenario.objectives.iter().map(|rule| rule.id))
                && scenario.objective_groups.iter().all(|old| {
                    origin
                        .scenario
                        .objective_groups
                        .iter()
                        .any(|group| group.id == old.id)
                })
        });
        let (Some(origin), None) = (matching.next(), matching.next()) else {
            continue;
        };
        for (old, current) in scenario
            .objectives
            .iter_mut()
            .zip(&origin.scenario.objectives)
        {
            old.prerequisite_objectives.clear();
            old.update_when_paused = current.update_when_paused;
        }
        scenario
            .objective_groups
            .clone_from(&origin.scenario.objective_groups);
        source.authored.merge(origin.authored.clone())?;
    }

    for binding in &mut source.document.scenario_script_bindings {
        let origins = scenario_origins
            .get(&binding.scenario)
            .map_or(&[][..], Vec::as_slice);
        let declares_binding = |origin: &&LegacyScenarioDeclaration| {
            let objective = if binding.objective == binding.scenario {
                origin.scenario.id
            } else {
                binding.objective
            };
            origin.bindings.iter().any(|authored| {
                authored.objective == objective
                    && authored.phase == binding.phase
                    && authored.entry == binding.entry
                    && AssetPath::new(&authored.script).key()
                        == AssetPath::new(&binding.script).key()
            })
        };
        let declared = origins.iter().filter(declares_binding).collect::<Vec<_>>();
        let paths = if declared.is_empty() {
            origins.iter().collect::<Vec<_>>()
        } else {
            declared
        };
        binding.script = resolve_legacy_script(
            archives,
            paths.into_iter().map(|origin| origin.path.as_path()),
            &binding.script,
        )?;
    }

    if !source.document.photo_challenge_script_bindings.is_empty() {
        let mut photo_origins = BTreeMap::<AssetId, Vec<PathBuf>>::new();
        for path in archives.source_paths_under(Path::new("photochall")) {
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            {
                continue;
            }
            let Ok(bytes) = archives.read_source(&path) else {
                continue;
            };
            let Ok(document) =
                parse_blue_fang_source_document(AssetPath::new(&path.to_string_lossy()), &bytes)
            else {
                continue;
            };
            let Ok((document, _)) =
                super::authored::lower_documents(&[&document], &[], &BTreeMap::new(), None)
            else {
                continue;
            };
            for photo in document.photo_challenge_script_bindings {
                photo_origins
                    .entry(photo.challenge)
                    .or_default()
                    .push(path.clone());
            }
        }
        source
            .document
            .photo_challenge_script_bindings
            .retain_mut(|binding| {
                let origins = photo_origins
                    .get(&binding.challenge)
                    .map_or(&[][..], Vec::as_slice);
                let Ok(path) = resolve_legacy_script(
                    archives,
                    origins.iter().map(PathBuf::as_path),
                    &binding.script,
                ) else {
                    return false;
                };
                binding.script = path;
                true
            });
    }
    source.authored.legacy_script_references = false;
    Ok(())
}

fn read_scenario_declaration(
    archives: &AssetArchives,
    path: &Path,
) -> Option<LegacyScenarioDeclaration> {
    let bytes = archives.read_source(path).ok()?;
    let document = parse_blue_fang_source_document(AssetPath::new(path.to_str()?), &bytes).ok()?;
    let (mut document, authored) =
        super::authored::lower_documents(&[&document], &[], &BTreeMap::new(), None).ok()?;
    let scenario = document.scenarios.pop()?;
    Some(LegacyScenarioDeclaration {
        path: path.to_path_buf(),
        scenario,
        bindings: document.scenario_script_bindings,
        authored,
    })
}

fn resolve_legacy_script<'a>(
    archives: &AssetArchives,
    origins: impl IntoIterator<Item = &'a Path>,
    reference: &str,
) -> io::Result<String> {
    let candidates = origins
        .into_iter()
        .filter_map(|origin| archives.resolve_script_reference(origin, reference))
        .collect::<BTreeSet<_>>();
    let path = if candidates.len() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("saved script {reference} has multiple declaring paths"),
        ));
    } else {
        candidates
            .into_iter()
            .next()
            .or_else(|| archives.resolve_script_reference(Path::new(""), reference))
    };
    path.map(|path| path.to_string_lossy().replace('\\', "/"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("saved script {reference} has no installed source"),
            )
        })
}
