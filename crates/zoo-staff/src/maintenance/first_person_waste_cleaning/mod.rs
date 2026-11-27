use std::time::Duration;

use bevy::prelude::*;
use openzt2_game_assets::{
    effect::SpawnCompleteParticleEffectDocument,
    world_definitions::{
        timing::TimekeeperEvent,
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::{
    camera::camera_runtime_state_types::{CameraMode, ZooCamera},
    immersive_modes::immersive_mode_state_types::{
        ActiveImmersiveMode, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
    },
    input::input_types::{ActionRequest, GameAction},
};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooTimekeeperElapsed,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    authored_float_attributes::AuthoredFloatAttributes,
    maintenance::maintenance_types::{
        CompostPooLevel, FacilityContainedWaste, FirstPersonStaffScript, RunFirstPersonStaffScript,
    },
    topology::gate_operation_types::Gate,
};

#[derive(Component, PartialEq)]
pub struct FirstPersonStaffActions(pub Vec<FirstPersonStaffAction>);

#[derive(Clone, PartialEq)]
pub struct FirstPersonStaffAction {
    pub operation: FirstPersonStaffOperation,
    pub localization: AssetId,
    pub effect: Option<AssetId>,
    pub attach_effect_with_rotation: bool,
    pub delay: Duration,
    pub prescript: Option<FirstPersonStaffScript>,
    pub postscript: Option<FirstPersonStaffScript>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FirstPersonStaffOperation {
    RemoveObject,
    SetNumber {
        property: AssetId,
        value: f32,
        threshold: f32,
    },
}

#[derive(Component)]
pub struct FirstPersonStaffAimOffset(pub f32);

#[derive(Component, Default, PartialEq, Eq)]
pub struct FirstPersonStaffAim {
    pub target: Option<Entity>,
    pub localization: Option<AssetId>,
}

#[derive(Event)]
pub struct PlayFirstPersonStaffEffect {
    pub avatar: Entity,
    pub target: Entity,
    pub effect: AssetId,
    pub position: Vec3,
    pub inherit_rotation: bool,
}

#[derive(Message)]
pub struct AdvanceFirstPersonStaffActions {
    pub delta: Duration,
}

#[derive(Component, Default)]
pub(super) struct PendingFirstPersonStaffActions(Vec<PendingStaffAction>);

struct PendingStaffAction {
    target: Entity,
    avatar: Entity,
    timer: Timer,
    action: FirstPersonStaffAction,
}

fn qualifying_action<'a>(
    actions: &'a FirstPersonStaffActions,
    gate: Option<&Gate>,
    waste: Option<&FacilityContainedWaste>,
    compost: Option<&CompostPooLevel>,
    attributes: Option<&AuthoredFloatAttributes>,
) -> Option<&'a FirstPersonStaffAction> {
    if gate.is_some_and(|gate| gate.open) {
        return None;
    }
    actions.0.iter().find(|action| match action.operation {
        FirstPersonStaffOperation::RemoveObject => true,
        FirstPersonStaffOperation::SetNumber {
            property,
            value,
            threshold,
        } => {
            let current = if property == AssetId::from_key("f_trashlevel") {
                waste.map(|waste| f32::from(waste.contained_waste_units))
            } else if property == AssetId::from_key("f_poolevel") {
                compost.map(|compost| compost.0)
            } else {
                None
            }
            .or_else(|| attributes.and_then(|attributes| attributes.get(property)));
            current.is_some_and(|current| (current - value).abs() > threshold)
        }
    })
}

fn choose_aim_target(
    camera: Vec3,
    aim: Vec3,
    targets: impl IntoIterator<Item = (Entity, Vec3)>,
) -> Option<Entity> {
    let mut nearest = None;
    let mut distance = f32::INFINITY;
    for (target, position) in targets {
        let offset = position - aim;
        if offset.abs().max_element() > 3.0 || offset.xz().length_squared() > 9.0 {
            continue;
        }
        let candidate = position.xz().distance_squared(camera.xz());
        if candidate < distance {
            nearest = Some(target);
            distance = candidate;
        }
    }
    nearest
}

pub(super) fn refresh_first_person_staff_aim_and_use(
    mut use_actions: MessageReader<ActionRequest>,
    cameras: Query<(&GlobalTransform, &CameraMode), With<ZooCamera>>,
    mut sessions: Query<(
        Entity,
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        &ImmersiveSubject,
        Option<&FirstPersonStaffAimOffset>,
        Option<&mut FirstPersonStaffAim>,
        Option<&mut PendingFirstPersonStaffActions>,
    )>,
    worlds: Query<&WorldMember>,
    targets: Query<(
        Entity,
        &GlobalTransform,
        &WorldMember,
        &FirstPersonStaffActions,
        Option<&Gate>,
        Option<&FacilityContainedWaste>,
        Option<&CompostPooLevel>,
        Option<&AuthoredFloatAttributes>,
    )>,
    mut commands: Commands,
) {
    let used = use_actions
        .read()
        .any(|request| request.action == GameAction::UseObject);
    use_actions.clear();
    for (session, mode, camera, avatar, offset, current_aim, pending) in &mut sessions {
        if mode.mode != ImmersiveMode::SuperStaff {
            continue;
        }
        let (Ok((pose, CameraMode::FirstPerson)), Ok(member)) =
            (cameras.get(camera.0), worlds.get(avatar.0))
        else {
            if let Some(mut current) = current_aim {
                current.set_if_neq(FirstPersonStaffAim::default());
            }
            continue;
        };
        let eye = pose.translation();
        let aim = eye + (pose.rotation() * Vec3::NEG_Z) * offset.map_or(1.0, |offset| offset.0);
        let nearest = choose_aim_target(
            eye,
            aim,
            targets.iter().filter_map(
                |(target, position, target_world, actions, gate, waste, compost, attributes)| {
                    if target_world.root != member.root
                        || pending.as_ref().is_some_and(|pending| {
                            pending.0.iter().any(|entry| entry.target == target)
                        })
                        || qualifying_action(actions, gate, waste, compost, attributes).is_none()
                    {
                        return None;
                    }
                    Some((target, position.translation()))
                },
            ),
        );
        let selected = nearest.and_then(|target| {
            let (_, position, _, actions, gate, waste, compost, attributes) =
                targets.get(target).ok()?;
            Some((
                target,
                position.translation(),
                qualifying_action(actions, gate, waste, compost, attributes)?,
            ))
        });
        let next = FirstPersonStaffAim {
            target: nearest,
            localization: selected.map(|(_, _, action)| action.localization),
        };
        if let Some(mut current) = current_aim {
            current.set_if_neq(next);
        } else {
            commands.entity(session).insert(next);
        }
        if !used {
            continue;
        }
        let Some((target, position, action)) = selected else {
            continue;
        };
        if let Some(effect) = action.effect {
            commands.trigger(PlayFirstPersonStaffEffect {
                avatar: avatar.0,
                target,
                effect,
                position,
                inherit_rotation: action.attach_effect_with_rotation,
            });
        }
        let entry = PendingStaffAction {
            target,
            avatar: avatar.0,
            timer: Timer::new(action.delay, TimerMode::Once),
            action: action.clone(),
        };
        if let Some(mut pending) = pending {
            pending.0.push(entry);
        } else {
            commands
                .entity(session)
                .insert(PendingFirstPersonStaffActions(vec![entry]));
        }
    }
}

pub(super) fn advance_first_person_staff_actions(
    mut updates: MessageReader<AdvanceFirstPersonStaffActions>,
    mut pending: Query<&mut PendingFirstPersonStaffActions>,
    live_targets: Query<(), With<FirstPersonStaffActions>>,
    mut commands: Commands,
) {
    for update in updates.read() {
        for mut pending in &mut pending {
            pending.0.retain_mut(|entry| {
                if !live_targets.contains(entry.target) {
                    return false;
                }
                if !entry.timer.tick(update.delta).is_finished() {
                    return true;
                }
                let action = entry.action.clone();
                let target = entry.target;
                let avatar = entry.avatar;
                commands.queue(move |world: &mut World| {
                    if let Some(script) = action.prescript {
                        world.trigger(RunFirstPersonStaffScript {
                            avatar,
                            target,
                            script,
                        });
                    }
                    match action.operation {
                        FirstPersonStaffOperation::RemoveObject => {
                            let _ = world.despawn(target);
                        }
                        FirstPersonStaffOperation::SetNumber {
                            property, value, ..
                        } => {
                            if property == AssetId::from_key("f_trashlevel") {
                                if let Some(mut waste) =
                                    world.get_mut::<FacilityContainedWaste>(target)
                                {
                                    let level = value
                                        .clamp(0.0, f32::from(waste.contained_waste_capacity_units))
                                        .round()
                                        as u16;
                                    waste
                                        .reborrow()
                                        .map_unchanged(|waste| &mut waste.contained_waste_units)
                                        .set_if_neq(level);
                                }
                            }
                            if property == AssetId::from_key("f_poolevel") {
                                if let Some(mut compost) = world.get_mut::<CompostPooLevel>(target)
                                {
                                    compost.set_if_neq(CompostPooLevel(value));
                                }
                            }
                            if let Some(mut attributes) =
                                world.get_mut::<AuthoredFloatAttributes>(target)
                            {
                                if attributes.get(property) != Some(value) {
                                    attributes.set(property, value);
                                }
                            } else if let Ok(mut target) = world.get_entity_mut(target) {
                                let mut attributes = AuthoredFloatAttributes::default();
                                attributes.set(property, value);
                                target.insert(attributes);
                            }
                        }
                    }
                    if let Some(script) = action.postscript {
                        world.trigger(RunFirstPersonStaffScript {
                            avatar,
                            target,
                            script,
                        });
                    }
                });
                false
            });
        }
    }
}

pub(super) fn hydrate_first_person_staff_actions(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut targets: Query<
        (
            Entity,
            Ref<DefinitionId>,
            Option<&mut FirstPersonStaffActions>,
        ),
        With<WorldMember>,
    >,
    mut commands: Commands,
) {
    let refresh_all = active.is_changed() || assets.is_changed();
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let remove = AssetId::from_key("removeobject");
    let set = AssetId::from_key("setattribute");
    for (entity, definition, current) in &mut targets {
        if !refresh_all && !definition.is_changed() {
            continue;
        }
        let next: Vec<_> = definitions.staff_interactions(definition.0).into_iter().flatten()
            .filter_map(|row| {
                let operation = if row.kind == remove {
                    FirstPersonStaffOperation::RemoveObject
                } else if row.kind == set {
                    FirstPersonStaffOperation::SetNumber {
                        property: row.property?, value: row.value, threshold: row.threshold,
                    }
                } else {
                    return None;
                };
                let script = |script: &openzt2_game_assets::world_definitions::interaction_policies::StaffInteractionScript| FirstPersonStaffScript {
                    context: script.context,
                    file: script.path.clone(),
                    function: script.entry.clone(),
                };
                Some(FirstPersonStaffAction {
                    operation,
                    localization: row.localization.unwrap_or_default(),
                    effect: row.effect,
                    attach_effect_with_rotation: row.use_parent_rotation,
                    delay: row.delay,
                    prescript: row.pre_script.as_ref().map(script),
                    postscript: row.post_script.as_ref().map(script),
                })
            }).collect();
        if next.is_empty() {
            if current.is_some() {
                commands.entity(entity).remove::<FirstPersonStaffActions>();
            }
        } else if let Some(mut current) = current {
            current.set_if_neq(FirstPersonStaffActions(next));
        } else {
            commands
                .entity(entity)
                .insert(FirstPersonStaffActions(next));
        }
    }
}

pub(super) fn publish_first_person_staff_action_delta(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut elapsed: MessageReader<ZooTimekeeperElapsed>,
    mut advances: MessageWriter<AdvanceFirstPersonStaffActions>,
) {
    let Some(policy) = active
        .get(&assets)
        .and_then(|definitions| definitions.authored_timekeeper_policy())
    else {
        elapsed.clear();
        return;
    };
    for event in elapsed
        .read()
        .filter(|event| event.event == TimekeeperEvent::Simulation)
    {
        advances.write(AdvanceFirstPersonStaffActions {
            delta: policy.simulation.interval.saturating_mul(event.count),
        });
    }
}

pub(super) fn play_first_person_staff_effect(
    effect: On<PlayFirstPersonStaffEffect>,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    poses: Query<&GlobalTransform>,
    mut requests: MessageWriter<SpawnCompleteParticleEffectDocument>,
    mut commands: Commands,
) {
    let mut transform = Transform::from_translation(effect.position);
    if effect.inherit_rotation {
        if let Ok(pose) = poses.get(effect.target) {
            transform.rotation = pose.rotation();
        }
    }
    if let Some(asset) = active
        .get(&assets)
        .and_then(|definitions| definitions.effect(effect.effect))
    {
        requests.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: asset,
            parent_entity: None,
            inherit_parent_rotation: false,
            effect_transform: transform,
            manual_particle_count: 0,
        });
    } else {
        commands.trigger(
            openzt2_game_assets::effect::PlayAuthoredParticleDictionaryEffect {
                effect: effect.effect,
                subject: effect.target,
                attach: false,
                inherit_parent_rotation: false,
                transform,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_cutoff_is_centred_ahead_but_chooses_by_distance_to_camera() {
        let nearer = Entity::from_bits(7);
        let farther = Entity::from_bits(8);
        assert_eq!(
            choose_aim_target(
                Vec3::ZERO,
                Vec3::X,
                [(farther, Vec3::X * 1.1), (nearer, Vec3::X * 0.2)]
            ),
            Some(nearer)
        );
    }

    #[test]
    fn an_empty_bin_action_requires_a_strict_threshold_crossing() {
        let actions = FirstPersonStaffActions(vec![FirstPersonStaffAction {
            operation: FirstPersonStaffOperation::SetNumber {
                property: AssetId::from_key("f_trashlevel"),
                value: 0.0,
                threshold: 20.0,
            },
            localization: AssetId::default(),
            effect: None,
            attach_effect_with_rotation: false,
            delay: Duration::ZERO,
            prescript: None,
            postscript: None,
        }]);
        let mut waste = FacilityContainedWaste {
            contained_waste_units: 20,
            contained_waste_capacity_units: 100,
        };
        assert!(qualifying_action(&actions, None, Some(&waste), None, None).is_none());
        waste.contained_waste_units = 21;
        assert!(qualifying_action(&actions, None, Some(&waste), None, None).is_some());
    }

    #[test]
    fn delayed_action_runs_its_prescript_before_removal_and_retires_missing_targets() {
        #[derive(Resource, Default)]
        struct Calls(Vec<Entity>);
        let mut app = App::new();
        app.init_resource::<Calls>()
            .add_message::<AdvanceFirstPersonStaffActions>()
            .add_systems(Update, advance_first_person_staff_actions)
            .add_observer(
                |event: On<RunFirstPersonStaffScript>,
                 mut calls: ResMut<Calls>,
                 targets: Query<(), With<FirstPersonStaffActions>>| {
                    assert!(targets.contains(event.target));
                    calls.0.push(event.target);
                },
            );
        let avatar = app.world_mut().spawn_empty().id();
        let dung = app
            .world_mut()
            .spawn(FirstPersonStaffActions(Vec::new()))
            .id();
        let retired = app
            .world_mut()
            .spawn(FirstPersonStaffActions(Vec::new()))
            .id();
        let action = FirstPersonStaffAction {
            operation: FirstPersonStaffOperation::RemoveObject,
            localization: AssetId::default(),
            effect: None,
            attach_effect_with_rotation: false,
            delay: Duration::from_secs(2),
            prescript: Some(FirstPersonStaffScript {
                context: AssetId::from_key("behavior"),
                file: "example.lua".into(),
                function: "clean".into(),
            }),
            postscript: None,
        };
        app.world_mut().spawn(PendingFirstPersonStaffActions(
            [dung, retired]
                .into_iter()
                .map(|target| PendingStaffAction {
                    target,
                    avatar,
                    timer: Timer::new(action.delay, TimerMode::Once),
                    action: action.clone(),
                })
                .collect(),
        ));
        let _ = app.world_mut().despawn(retired);
        app.world_mut()
            .write_message(AdvanceFirstPersonStaffActions {
                delta: Duration::from_millis(1990),
            });
        app.update();
        assert!(app.world().resource::<Calls>().0.is_empty());
        assert!(app.world().get_entity(dung).is_ok());
        app.world_mut()
            .write_message(AdvanceFirstPersonStaffActions {
                delta: Duration::from_millis(10),
            });
        app.update();
        assert_eq!(app.world().resource::<Calls>().0, [dung]);
        assert!(app.world().get_entity(dung).is_err());
    }
}
