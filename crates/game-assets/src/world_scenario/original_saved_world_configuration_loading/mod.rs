use std::{io, path::Path};

use bevy::asset::LoadContext;
use openzt2_game_data::{world_scenario::WorldScenarioDocument, AssetId};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    },
};

use super::{
    starting_zoo::{at, child, required, required_f32},
    world_source_document_dependency_lowering::lower_world_source_document_and_resolve_dependencies,
};

pub(crate) async fn load_saved_world_configuration(
    saved: &OrderedSourceDocument,
    saved_map: AssetId,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<(WorldScenarioDocument, bool)> {
    let root = child(&saved.root, "BFContext")
        .ok_or_else(|| at(saved, &saved.root, "saved world has no BFContext"))?;
    let template = load_document_reference(
        archives,
        context,
        saved.path.as_str(),
        required(saved, root, "templateFile")?,
    )
    .await?;
    let economy = child(&template.root, "ZTEconomyMgr").ok_or_else(|| {
        at(
            &template,
            &template.root,
            "world template has no ZTEconomyMgr",
        )
    })?;
    let economy_config = load_document_reference(
        archives,
        context,
        template.path.as_str(),
        required(&template, economy, "configData")?,
    )
    .await?;
    let unlimited_cash = match required(&economy_config, &economy_config.root, "active")? {
        "true" => false,
        "false" => true,
        _ => {
            return Err(at(
                &economy_config,
                &economy_config.root,
                "economy active is not a canonical boolean",
            ))
        }
    };
    let manager = child(root, "ZTScenarioMgr")
        .ok_or_else(|| at(saved, root, "saved world has no ZTScenarioMgr"))?;
    let goals = load_document_reference(
        archives,
        context,
        saved.path.as_str(),
        required(saved, manager, "context")?,
    )
    .await?;
    let (mut document, _) =
        lower_world_source_document_and_resolve_dependencies(&goals, archives, context).await?;
    if document.scenarios.len() != 1 {
        return Err(at(
            &goals,
            &goals.root,
            "saved scenario context must define one scenario",
        ));
    }
    let previous_id = document.scenarios[0].id;
    document.scenarios[0].id = saved_map;
    document.scenarios[0].evaluation_interval_seconds = required_f32(saved, manager, "interval")?;
    for binding in &mut document.scenario_script_bindings {
        if binding.scenario != previous_id {
            return Err(at(
                &goals,
                &goals.root,
                "scenario script binding belongs to a different scenario",
            ));
        }
        binding.scenario = saved_map;
        if binding.objective == previous_id {
            binding.objective = saved_map;
        }
    }
    let always_run =
        super::authored::lower_scenario_manager_always_run_bindings(saved, manager, saved_map)?;
    document.scenario_script_bindings.retain(|binding| {
        binding.phase != openzt2_game_data::world_scenario::ScenarioScriptPhase::AlwaysRun
    });
    document.scenario_script_bindings.extend(always_run);
    Ok((document, unlimited_cash))
}

async fn load_document_reference(
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
    source: &str,
    reference: &str,
) -> io::Result<OrderedSourceDocument> {
    let path = archives
        .resolve_source_reference(Path::new(source), reference)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{source} references missing document {reference}"),
            )
        })?;
    let bytes = context
        .read_asset_bytes(path.clone())
        .await
        .map_err(io::Error::other)?;
    parse_blue_fang_source_document(AssetPath::new(&path.to_string_lossy()), &bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
