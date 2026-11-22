use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_assets::{
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
        ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_zoo_state::{
    guests::guest_simulation_types::{Guest, GuestRng, VisitTime},
    need_adjustment::advance_need_adjustment_timer,
    shows::running_show_types::GuestShowNeed,
};

#[derive(Asset, TypePath)]
pub(super) struct ShowNeedPolicy {
    initial: [f32; 2],
    trigger: f32,
    cessation: f32,
    adjustment: f32,
}

#[derive(Default, TypePath)]
pub(super) struct ShowNeedPolicyLoader;

impl AssetLoader for ShowNeedPolicyLoader {
    type Asset = ShowNeedPolicy;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<ShowNeedPolicy> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let source = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let adjustments = find(&source.root, "BFAINeedAdjusts", None)?;
        let state = find(&source.root, "BFAIStateVar", Some("show"))?;
        Ok(ShowNeedPolicy {
            initial: [number(state, "ValueMin")?, number(state, "ValueMax")?],
            trigger: number(state, "TriggerThreshold")?,
            cessation: number(state, "CessationThreshold")?,
            adjustment: number(adjustments, "show")?,
        })
    }
}

fn find<'a>(
    node: &'a OrderedSourceDocumentNode,
    element: &str,
    name: Option<&str>,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    if node.name.eq_ignore_ascii_case(element)
        && name.is_none_or(|name| node.attribute_named_any(&["Name"]) == Some(name))
    {
        return Ok(node);
    }
    node.element_children()
        .find_map(|child| find(child, element, name).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {element}")))
}

fn number(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<f32> {
    node.attribute_named_any(&[name])
        .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
        .filter(|number| number.is_finite())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("invalid {name}")))
}

#[derive(Resource)]
pub(super) struct ShowNeedSource(Handle<ShowNeedPolicy>);

pub(super) fn load_show_need_policy(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ShowNeedSource(
        assets.load("entities/units/guests/ai/guest.xml"),
    ));
}

pub(super) fn advance_show_needs(
    source: Res<ShowNeedSource>,
    policies: Res<Assets<ShowNeedPolicy>>,
    time: Res<Time<Fixed>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<(Entity, &GuestRng, &VisitTime, Option<&mut GuestShowNeed>), With<Guest>>,
    mut commands: Commands,
) {
    let (Some(policy), Some(timer)) = (
        policies.get(&source.0),
        definitions
            .get(&assets)
            .and_then(|definitions| definitions.need_adjustment()),
    ) else {
        return;
    };
    let step_ns = u64::try_from(time.timestep().as_nanos()).unwrap_or(u64::MAX);
    for (guest, rng, visit, need) in &mut guests {
        if let Some(mut need) = need {
            let need = &mut *need;
            if timer.enabled
                && advance_need_adjustment_timer(
                    &mut need.remaining_ns,
                    step_ns,
                    timer.delay_steps,
                    &mut need.rng,
                )
            {
                need.adjust(policy.adjustment);
            }
        } else {
            let mut rng = rng.0;
            let [min, max] = policy.initial;
            let value = min + rng.unit_f32() * (max - min);
            commands.entity(guest).insert(GuestShowNeed {
                value,
                triggered: value >= policy.trigger,
                minimum: policy.cessation,
                cessation: policy.cessation,
                trigger: policy.trigger,
                remaining_ns: visit.need_adjustment_remaining_ns,
                rng,
            });
        }
    }
}
