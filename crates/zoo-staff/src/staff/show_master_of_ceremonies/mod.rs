use std::time::Duration;

use avian3d::prelude::RigidBody;
use bevy::prelude::*;
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_shows::shows::ShowMasterOfCeremoniesCue;
use openzt2_world::world_spawn::prefab_world_instance_spawning::{
    morph_fade_projection::PrefabMorphFade, spawn_loaded_scene_prefab_as_world_instance,
};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
        world_membership_types::WorldMember,
    },
};
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::BehaviorSelfToken,
    behavior_task_execution_types::abandon_behavior_task_for_reconsideration,
    shows::{running_show_types::ShowPhase, show_stage_types::ShowStage},
};

#[derive(Component)]
struct StagePerformer(Entity);

#[derive(Component)]
#[relationship(relationship_target = PerformersForStage)]
struct PerformsAtStage(Entity);

#[derive(Component)]
#[relationship_target(relationship = PerformsAtStage, linked_spawn)]
struct PerformersForStage(Vec<Entity>);

#[derive(Component)]
struct PendingStageCue(ShowMasterOfCeremoniesCue);

#[derive(Component)]
struct StagePerformerRetirement {
    begin_fade: u64,
    end_fade: u64,
    fade: Duration,
}

pub(super) fn register(app: &mut App) {
    use openzt2_world_state::application_schedule::FixedGameSet;

    app.add_message::<ShowMasterOfCeremoniesCue>().add_systems(
        FixedUpdate,
        (
            receive_stage_cues,
            run_stage_cues
                .after(receive_stage_cues)
                .before(super::idle_staff_task_selection::start_staff_token_tasks),
            retire_stage_performers.after(run_stage_cues),
        )
            .in_set(FixedGameSet::Think),
    );
}

fn receive_stage_cues(
    mut cues: MessageReader<ShowMasterOfCeremoniesCue>,
    mut stages: Query<Option<&mut PendingStageCue>, With<ShowStage>>,
    mut commands: Commands,
) {
    for cue in cues.read() {
        let Ok(pending) = stages.get_mut(cue.stage) else {
            continue;
        };
        let next = PendingStageCue(ShowMasterOfCeremoniesCue {
            run: cue.run,
            stage: cue.stage,
            phase: cue.phase,
            attachment: cue.attachment,
            performer_type: cue.performer_type,
            token: cue.token,
            fade_ns: cue.fade_ns,
            replace_timeout_ns: cue.replace_timeout_ns,
        });
        if let Some(mut pending) = pending {
            *pending = next;
        } else {
            commands.entity(cue.stage).insert(next);
        }
    }
}

fn stage_ancestor(mut entity: Entity, parents: &Query<&ChildOf>, stage: Entity) -> bool {
    while let Ok(parent) = parents.get(entity) {
        entity = parent.parent();
        if entity == stage {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_stage_cues(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<bevy::gltf::Gltf>>,
    clocks: Query<&ZooClock>,
    mut identifiers: Query<&mut PersistentIdAllocator>,
    mut stages: Query<
        (
            Entity,
            &WorldMember,
            &PendingStageCue,
            Option<&mut StagePerformer>,
        ),
        With<ShowStage>,
    >,
    performers: Query<Option<&Children>, With<PerformsAtStage>>,
    tokens: Query<&BehaviorSelfToken>,
    sockets: Query<(
        Entity,
        &PrefabAuthoredAttachmentIdentifier,
        &GlobalTransform,
    )>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Some(step) = definitions.simulation_timestep() else {
        return;
    };
    let ticks = |ns| u64::try_from(u128::from(ns).div_ceil(step.as_nanos())).unwrap_or(u64::MAX);
    for (stage, member, cue, current) in &mut stages {
        let cue = &cue.0;
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let retirement = StagePerformerRetirement {
            begin_fade: clock.tick.saturating_add(ticks(cue.replace_timeout_ns)),
            end_fade: clock
                .tick
                .saturating_add(ticks(cue.replace_timeout_ns))
                .saturating_add(ticks(cue.fade_ns)),
            fade: Duration::from_nanos(cue.fade_ns),
        };
        let current_entity = current.as_ref().map(|current| current.0);
        let performer = if let Some(existing) =
            current_entity.filter(|&actor| performers.contains(actor))
        {
            if let Ok(Some(children)) = performers.get(existing) {
                for token in children.iter().filter(|&child| tokens.contains(child)) {
                    commands.entity(token).try_despawn();
                }
            }
            abandon_behavior_task_for_reconsideration(existing, &mut commands);
            commands
                .entity(existing)
                .remove::<(StagePerformerRetirement, PrefabMorphFade)>();
            existing
        } else {
            if cue.phase == ShowPhase::BetweenShows {
                commands.entity(stage).remove::<PendingStageCue>();
                continue;
            }
            let Some(pose) = sockets.iter().find_map(|(entity, socket, pose)| {
                (socket.0 == cue.attachment && stage_ancestor(entity, &parents, stage))
                    .then(|| pose.compute_transform())
            }) else {
                continue;
            };
            let Some((definition, prefab_handle)) = definitions
                .objects()
                .filter(|object| {
                    object.id == cue.performer_type
                        || object
                            .behavior_type_identifiers
                            .contains(&cue.performer_type)
                })
                .filter(|object| definitions.find_staff_by_object(object.id).is_some())
                .find_map(|object| Some((object.id, definitions.scene(object.prefab)?)))
            else {
                continue;
            };
            let Some(prefab) = prefabs.get(&prefab_handle) else {
                continue;
            };
            if openzt2_world_state::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
                continue;
            }
            let Some(id) = identifiers
                .get_mut(member.root)
                .ok()
                .and_then(|mut allocator| allocator.allocate().ok())
            else {
                continue;
            };
            let actor = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                prefab_handle,
                member.root,
                definition,
                id,
                pose,
                true,
                None,
                RigidBody::Kinematic,
            );
            commands.entity(actor).insert((
                PerformsAtStage(stage),
                PrefabMorphFade::fade_in(Duration::from_nanos(cue.fade_ns)),
            ));
            if let Some(mut current) = current {
                current.0 = actor;
            } else {
                commands.entity(stage).insert(StagePerformer(actor));
            }
            actor
        };
        commands.spawn((
            ChildOf(performer),
            BehaviorSelfToken {
                name: cue.token,
                available_at: clock.tick,
                expires_at: retirement.begin_fade,
                payload: Some(stage),
                retain_on_failure: false,
            },
        ));
        if cue.phase == ShowPhase::BetweenShows {
            commands.entity(performer).insert(retirement);
        }
        commands.entity(stage).remove::<PendingStageCue>();
    }
}

fn retire_stage_performers(
    clocks: Query<&ZooClock>,
    performers: Query<
        (
            Entity,
            &WorldMember,
            &StagePerformerRetirement,
            Has<PrefabMorphFade>,
        ),
        With<PerformsAtStage>,
    >,
    mut commands: Commands,
) {
    for (actor, member, retirement, fading) in &performers {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        if clock.tick >= retirement.end_fade {
            commands.entity(actor).try_despawn();
        } else if clock.tick >= retirement.begin_fade && !fading {
            commands
                .entity(actor)
                .insert(PrefabMorphFade::fade_out(retirement.fade));
        }
    }
}
