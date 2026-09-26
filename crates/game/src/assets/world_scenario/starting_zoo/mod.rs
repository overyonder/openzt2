//! Direct lowering of one shipped starting-zoo document.

use crate::assets::world_scenario::world_scenario_source_reference_resolution::SourceReference;

mod entities;

use std::{collections::BTreeMap, io};

use openzt2_game_data::{
    world_definitions::simulation_time::SimulationTimingDefinition,
    world_scenario::StartingZooRecord, AssetId,
};

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};

pub(super) fn lower(
    document: &OrderedSourceDocument,
    dependencies: &[SourceReference],
    actor_scene_paths: &BTreeMap<String, String>,
    timing: &SimulationTimingDefinition,
) -> io::Result<StartingZooRecord> {
    let map = map_id(document)?;
    let roots = document.root.element_children().collect::<Vec<_>>();
    let status = descendants(&roots)
        .find(|node| local(&node.name) == "ZTStatus")
        .ok_or_else(|| at_root(document, "starting zoo has no ZTStatus"))?;
    let economy = descendants(&roots)
        .find(|node| local(&node.name) == "BFGEconomyData" && node.attribute("cash").is_some())
        .ok_or_else(|| at_root(document, "starting zoo has no cash-bearing BFGEconomyData"))?;
    let world = descendants(&roots)
        .find(|node| local(&node.name) == "ZTWorldMgr")
        .ok_or_else(|| at_root(document, "starting zoo has no ZTWorldMgr"))?;
    let camera = child(world, "camPos")
        .ok_or_else(|| at(document, world, "starting zoo ZTWorldMgr has no camPos"))?;
    let admissions_open = required(document, status, "zoo_is_open")?;
    if !matches!(admissions_open, "true" | "false") {
        return Err(at(
            document,
            status,
            "ZTStatus zoo_is_open is not a canonical boolean",
        ));
    }
    let name = required(document, status, "zooName")?;
    if name.is_empty() {
        return Err(at(document, status, "ZTStatus zooName is empty"));
    }
    let fame_half_stars = required(document, status, "num_halfstars")?
        .parse::<u8>()
        .map_err(|_| {
            at(
                document,
                status,
                "ZTStatus num_halfstars is not an unsigned byte",
            )
        })?;
    let mut starting_zoo_record = StartingZooRecord {
        id: AssetId::from_key(&format!("start:{map}")),
        profile: AssetId::default(),
        map: AssetId::from_key(&map),
        camera_position_m: convert_source_z_up_vector_to_bevy_y_up_coordinates([
            required_f32(document, camera, "x")?,
            required_f32(document, camera, "y")?,
            required_f32(document, camera, "z")?,
        ]),
        camera_rotation_radians: [
            required_f32(document, world, "camPitch")?,
            required_f32(document, world, "camYaw")?,
            required_f32(document, world, "camRoll")?,
        ],
        name: name.to_owned(),
        admissions_open: admissions_open == "true",
        cash_cents: cents(document, economy, "cash")?,
        admission_cents: status
            .attribute("adultAdmission")
            .map(|_| cents(document, status, "adultAdmission"))
            .transpose()?,
        fame_half_stars,
        maximum_fame_percent_reached: status
            .attribute("zoo_fame_max_reached")
            .map(|_| {
                let maximum = required_f32(document, status, "zoo_fame_max_reached")?;
                if !(0.0..=100.0).contains(&maximum) {
                    return Err(at(
                        document,
                        status,
                        "zoo_fame_max_reached is outside 0..100",
                    ));
                }
                Ok(maximum)
            })
            .transpose()?,
        start_tick: 0,
        absolute_day: 0,
        calendar: [
            timing.calendar.epoch_year,
            u16::from(timing.calendar.epoch_month),
            u16::from(timing.calendar.epoch_day),
        ],
        imported_seed: None,
        entities: Vec::new(),
        spawn_values: Vec::new(),
        paths: Vec::new(),
        topology_nodes: Vec::new(),
        fences: Vec::new(),
    };
    entities::lower_starting_zoo_entities_paths_and_fences_into_record(
        document,
        dependencies,
        actor_scene_paths,
        &mut starting_zoo_record,
    )?;
    Ok(starting_zoo_record)
}

fn cents(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    attribute: &str,
) -> io::Result<i64> {
    let source = required(document, node, attribute)?;
    let dollars = source.parse::<f64>().map_err(|_| {
        at(
            document,
            node,
            format!("{attribute} is not a finite currency value: {source}"),
        )
    })?;
    let value = (dollars * 100.0).round();
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err(at(
            document,
            node,
            format!("{attribute} cannot be represented as cents: {source}"),
        ));
    }
    Ok(value as i64)
}

fn map_id(document: &OrderedSourceDocument) -> io::Result<String> {
    document
        .path
        .key()
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem))
        .filter(|stem| !stem.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| at_root(document, "starting zoo path has no map id"))
}

pub(super) fn descendants<'a>(
    roots: &'a [&'a OrderedSourceDocumentNode],
) -> impl Iterator<Item = &'a OrderedSourceDocumentNode> + 'a {
    let mut stack = roots.iter().rev().copied().collect::<Vec<_>>();
    std::iter::from_fn(move || {
        let next = stack.pop()?;
        stack.extend(next.element_children().rev());
        Some(next)
    })
}

pub(super) fn child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children()
        .find(|child| local(&child.name) == name)
}

pub(super) fn required<'a>(
    document: &OrderedSourceDocument,
    node: &'a OrderedSourceDocumentNode,
    attribute: &str,
) -> io::Result<&'a str> {
    node.attribute(attribute).ok_or_else(|| {
        at(
            document,
            node,
            format!("<{}> is missing {attribute}", local(&node.name)),
        )
    })
}

pub(super) fn required_f32(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    attribute: &str,
) -> io::Result<f32> {
    let source = required(document, node, attribute)?;
    source
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            at(
                document,
                node,
                format!("{attribute} is not a finite f32: {source}"),
            )
        })
}

pub(super) fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(super) fn at(
    document: &OrderedSourceDocument,
    node: &OrderedSourceDocumentNode,
    message: impl std::fmt::Display,
) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{}:{}..{}: {message}",
            document.path.key(),
            node.span.start,
            node.span.end
        ),
    )
}

fn at_root(document: &OrderedSourceDocument, message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{}:{}..{}: {message}",
            document.path.key(),
            document.root.span.start,
            document.root.span.end
        ),
    )
}
