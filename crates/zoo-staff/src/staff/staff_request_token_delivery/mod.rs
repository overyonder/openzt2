use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffTokenDispatchRule, AssetId};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{
        BehaviorSelfToken, BehaviorTokenContext, BehaviorTokenXmlPayload,
    },
    staff::staff_job_types::JobClaim,
};
use std::collections::HashMap;

#[derive(Component)]
#[relationship(relationship_target = StaffRequestTokens)]
pub(super) struct StaffRequestTokenOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = StaffRequestTokenOf, linked_spawn)]
pub(super) struct StaffRequestTokens(Vec<Entity>);

#[derive(Component, Default, PartialEq)]
pub struct StaffTokenRoutingSettings(pub HashMap<(AssetId, usize), StaffTokenRoutingRow>);

#[derive(Clone, Copy, PartialEq)]
pub struct StaffTokenRoutingRow {
    pub chance: u16,
    pub radius: Option<f32>,
    pub chance_in_radius: u16,
    pub delay_ticks: u64,
    pub duration_ticks: u64,
    pub point: Option<Vec3>,
}

impl Default for StaffTokenRoutingRow {
    fn default() -> Self {
        Self {
            chance: 1000,
            radius: None,
            chance_in_radius: 1000,
            delay_ticks: 0,
            duration_ticks: u64::MAX,
            point: None,
        }
    }
}

#[derive(Component, Default)]
pub(super) struct StaffTokenTemplatePoints(HashMap<(AssetId, usize), Vec3>);

pub(super) fn project_staff_token_routing_settings(
    active: Res<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    assets: Res<Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    mut roots: Query<
        (Entity, Option<&mut StaffTokenRoutingSettings>),
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    mut commands: Commands,
) {
    let refresh = active.is_changed() || assets.is_changed();
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Some(step) = definitions.simulation_timestep() else {
        return;
    };
    let Some(manager) = definitions.staff_manager() else {
        return;
    };
    let to_ticks = |seconds: f32| {
        if seconds == -1.0 {
            return (1_000_000_000.0 / step.as_secs_f64()).ceil() as u64;
        }
        (f64::from(seconds.max(0.0)) / step.as_secs_f64()).ceil() as u64
    };
    let percent = |value: Option<f32>| value.unwrap_or(1000.0).ceil().clamp(0.0, 1000.0) as u16;
    for (root, current) in &mut roots {
        if !refresh && current.is_some() {
            continue;
        }
        let mut ordinals = HashMap::<AssetId, usize>::new();
        let next = StaffTokenRoutingSettings(
            manager
                .tokens
                .iter()
                .map(|rule| {
                    let source = &rule.routing;
                    let route = StaffTokenRoutingRow {
                        chance: percent(source.chance_percent),
                        radius: source.radius_m,
                        chance_in_radius: percent(source.chance_in_radius_percent),
                        delay_ticks: source
                            .time_to_activation_seconds
                            .or(source.time_in_seconds)
                            .map_or(0, to_ticks),
                        duration_ticks: source
                            .time_left_seconds
                            .or(source.timeout_seconds)
                            .map_or(u64::MAX, to_ticks),
                        point: source.point_m.map(Vec3::from_array),
                    };
                    let ordinal = ordinals.entry(rule.token).or_default();
                    let entry = ((rule.token, *ordinal), route);
                    *ordinal += 1;
                    entry
                })
                .collect(),
        );
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(root).insert(next);
        }
    }
}

pub(super) fn remove_staff_request_tokens(job: Entity, commands: &mut Commands) {
    commands.queue(move |world: &mut World| {
        let tokens: Vec<Entity> = world
            .get::<StaffRequestTokens>(job)
            .into_iter()
            .flat_map(|tokens| tokens.iter())
            .collect();
        for token in tokens {
            let _ = world.despawn(token);
        }
    });
}

pub(super) fn clear_tokens_for_released_claim(
    removed: On<Remove<JobClaim>>,
    links: Query<&StaffRequestTokens>,
    mut commands: Commands,
) {
    if let Ok(tokens) = links.get(removed.entity) {
        for token in tokens.iter() {
            commands.entity(token).try_despawn();
        }
    }
}

pub(super) struct StaffTokenDispatch<'a> {
    pub(super) job: Entity,
    pub(super) worker: Entity,
    pub(super) target: Option<Entity>,
    pub(super) point: Option<Vec3>,
    pub(super) key: AssetId,
    pub(super) rules: &'a [StaffTokenDispatchRule],
    pub(super) settings: Option<&'a StaffTokenRoutingSettings>,
    pub(super) now: u64,
}

pub(super) fn deliver_staff_request_tokens(
    dispatch: StaffTokenDispatch<'_>,
    template_points: &mut StaffTokenTemplatePoints,
    mut named_recipients: impl FnMut(AssetId, Option<f32>) -> Vec<Entity>,
    mut draw: impl FnMut() -> u16,
    commands: &mut Commands,
) -> Option<(Entity, AssetId)> {
    let subject_route = AssetId::from_key("subject");
    let target_route = AssetId::from_key("target");
    let mut first_worker_token = None;
    for (ordinal, rule) in dispatch
        .rules
        .iter()
        .filter(|rule| rule.token == dispatch.key)
        .enumerate()
    {
        let Some(name) = rule.name else {
            continue;
        };
        let identity = (dispatch.key, ordinal);
        let settings = dispatch
            .settings
            .and_then(|settings| settings.0.get(&identity))
            .copied()
            .unwrap_or_default();
        if !passes_token_chance(draw(), settings.chance) {
            continue;
        }
        let recipients = match rule.give_to {
            Some(route) if route == subject_route => vec![dispatch.worker],
            Some(route) if route == target_route => dispatch.target.into_iter().collect(),
            Some(route) => named_recipients(route, settings.radius)
                .into_iter()
                .filter(|&recipient| {
                    recipient != dispatch.worker
                        && passes_token_chance(draw(), settings.chance_in_radius)
                })
                .collect(),
            None => Vec::new(),
        };
        let payload = match rule.payload {
            Some(route) if route == subject_route => Some(dispatch.worker),
            Some(route) if route == target_route => dispatch.target,
            _ => None,
        };
        for recipient in recipients {
            let point = match dispatch.point {
                Some(point) => template_points.0.insert(identity, point).or(settings.point),
                None => template_points.0.get(&identity).copied().or(settings.point),
            };
            let start = dispatch.now.saturating_add(settings.delay_ticks);
            let mut token = commands.spawn((
                ChildOf(recipient),
                StaffRequestTokenOf(dispatch.job),
                BehaviorTokenContext {
                    force: rule.force.unwrap_or(false),
                    point,
                },
                BehaviorSelfToken {
                    name,
                    available_at: start,
                    expires_at: start.saturating_add(settings.duration_ticks),
                    payload,
                    retain_on_failure: false,
                },
            ));
            if let Some(xml) = &rule.routing.xml_payload {
                token.insert(BehaviorTokenXmlPayload(xml.clone()));
            }
            if recipient == dispatch.worker && first_worker_token.is_none() {
                first_worker_token = Some((token.id(), name));
            }
        }
    }
    first_worker_token
}

fn passes_token_chance(draw: u16, chance: u16) -> bool {
    draw % 100 < chance
}

#[derive(Component)]
pub(super) struct StaffRequestExecutionToken(pub(super) Entity);

#[cfg(test)]
mod tests {
    use super::*;

    fn subject_rule(key: AssetId, name: &str, force: bool) -> StaffTokenDispatchRule {
        StaffTokenDispatchRule {
            token: key,
            name: Some(AssetId::from_key(name)),
            give_to: Some(AssetId::from_key("subject")),
            payload: Some(AssetId::from_key("target")),
            force: Some(force),
            routing: Default::default(),
        }
    }

    #[test]
    fn two_authored_rows_roll_in_order_and_keep_their_own_names() {
        let mut world = World::new();
        let worker = world.spawn_empty().id();
        let target = world.spawn_empty().id();
        let job = world.spawn_empty().id();
        let key = AssetId::from_key("care_route");
        let rules = [
            subject_rule(key, "t_feedanimal", true),
            subject_rule(key, "t_cureanimal", true),
        ];
        let mut settings = StaffTokenRoutingSettings::default();
        settings.0.insert(
            (key, 1),
            StaffTokenRoutingRow {
                chance: 99,
                ..default()
            },
        );
        let mut draws = [0, 99].into_iter();
        let token = deliver_staff_request_tokens(
            StaffTokenDispatch {
                job,
                worker,
                target: Some(target),
                point: None,
                key,
                rules: &rules,
                settings: Some(&settings),
                now: 0,
            },
            &mut StaffTokenTemplatePoints::default(),
            |_, _| panic!("subject route made a named query"),
            || draws.next().unwrap(),
            &mut world.commands(),
        )
        .unwrap();
        assert_eq!(draws.next(), None);
        world.flush();
        assert_eq!(world.query::<&BehaviorSelfToken>().iter(&world).count(), 1);
        let row = world.get::<BehaviorSelfToken>(token.0).unwrap();
        assert_eq!(token.1, row.name);
        assert_eq!(row.name, AssetId::from_key("t_feedanimal"));
        assert_eq!(row.payload, Some(target));
        assert!(world.get::<BehaviorTokenContext>(token.0).unwrap().force);
    }

    #[test]
    fn point_delivery_copies_the_template_before_replacing_its_position() {
        let mut world = World::new();
        let worker = world.spawn_empty().id();
        let job = world.spawn_empty().id();
        let key = AssetId::from_key("t_cleantank");
        let rules = [subject_rule(key, "t_cleantank", true)];
        let mut points = StaffTokenTemplatePoints::default();
        let [first, next] = [Vec3::X, Vec3::Z].map(|point| {
            deliver_staff_request_tokens(
                StaffTokenDispatch {
                    job,
                    worker,
                    target: None,
                    point: Some(point),
                    key,
                    rules: &rules,
                    settings: None,
                    now: 0,
                },
                &mut points,
                |_, _| Vec::new(),
                || 0,
                &mut world.commands(),
            )
            .unwrap()
        });
        world.flush();
        assert_eq!(
            world.get::<BehaviorTokenContext>(first.0).unwrap().point,
            None
        );
        assert_eq!(
            world.get::<BehaviorTokenContext>(next.0).unwrap().point,
            Some(Vec3::X)
        );
    }

    #[test]
    fn replacement_retires_old_routed_tokens_before_the_new_delivery() {
        let mut world = World::new();
        let old = world.spawn_empty().id();
        let new = world.spawn_empty().id();
        let target = world.spawn_empty().id();
        let job = world.spawn_empty().id();
        let key = AssetId::from_key("care_route");
        let mut points = StaffTokenTemplatePoints::default();
        let old_token = deliver_staff_request_tokens(
            StaffTokenDispatch {
                job,
                worker: old,
                target: Some(target),
                point: None,
                key,
                rules: &[subject_rule(key, "t_crateanimal", false)],
                settings: None,
                now: 0,
            },
            &mut points,
            |_, _| Vec::new(),
            || 0,
            &mut world.commands(),
        )
        .unwrap();
        world.flush();
        assert!(
            !world
                .get::<BehaviorTokenContext>(old_token.0)
                .unwrap()
                .force
        );
        let mut commands = world.commands();
        remove_staff_request_tokens(job, &mut commands);
        let new_token = deliver_staff_request_tokens(
            StaffTokenDispatch {
                job,
                worker: new,
                target: Some(target),
                point: None,
                key,
                rules: &[subject_rule(key, "t_feedanimal", true)],
                settings: None,
                now: 0,
            },
            &mut points,
            |_, _| Vec::new(),
            || 0,
            &mut commands,
        )
        .unwrap();
        world.flush();
        assert!(world.get_entity(old_token.0).is_err());
        assert!(
            world
                .get::<BehaviorTokenContext>(new_token.0)
                .unwrap()
                .force
        );
        assert_eq!(
            world.get::<BehaviorSelfToken>(new_token.0).unwrap().payload,
            Some(target)
        );
        assert_eq!(world.get::<ChildOf>(new_token.0).unwrap().parent(), new);
    }
}
