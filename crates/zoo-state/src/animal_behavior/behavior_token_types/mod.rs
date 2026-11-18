use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use openzt2_game_data::AssetId;

/// ChildOf names the recipient; a running task may claim this token as its object.
#[derive(Component)]
#[component(on_insert = register_behavior_token_expiration)]
pub struct BehaviorSelfToken {
    pub name: AssetId,
    pub available_at: u64,
    pub expires_at: u64,
    /// The authored `Payload` participant, the target of a `fromToken` task.
    pub payload: Option<Entity>,
    /// Whether a failed claiming task leaves the token in place.
    pub retain_on_failure: bool,
}

#[derive(Component)]
pub struct BehaviorTokenExpires;

fn register_behavior_token_expiration(mut world: DeferredWorld, context: HookContext) {
    let Some(token) = world.get::<BehaviorSelfToken>(context.entity) else {
        return;
    };
    let finite = token.expires_at != u64::MAX;
    if finite == world.get::<BehaviorTokenExpires>(context.entity).is_some() {
        return;
    }
    let mut commands = world.commands();
    let mut token = commands.entity(context.entity);
    if finite {
        token.try_insert(BehaviorTokenExpires);
    } else {
        token.try_remove::<BehaviorTokenExpires>();
    }
}

#[derive(Component)]
pub struct ClaimedBehaviorSelfToken {
    pub execution_id: u64,
}
