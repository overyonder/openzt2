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

impl BehaviorSelfToken {
    pub const fn is_available_at(&self, tick: u64) -> bool {
        self.available_at <= tick && tick < self.expires_at
    }
}

/// Point and force authored for this delivery. The token's `ChildOf` is
/// the recipient, and its entity payload can name a different participant.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct BehaviorTokenContext {
    pub force: bool,
    pub point: Option<Vec3>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorTokenXmlPayload(
    pub openzt2_game_data::world_definitions::staff_management::StaffTokenXmlElement,
);

/// A delivery asked its recipient to reconsider when the token becomes available.
#[derive(Component)]
pub struct PendingBehaviorTokenReconsideration;

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

/// Coordinates carried by a trick token, in world units.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BehaviorTrickDestination(pub Vec3);

/// Placement position to which a show prop's authored offsets are relative.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BehaviorTrickPropOrigin(pub Vec3);

/// An authored transaction's token-carried price and accounting category.
#[derive(Component, Clone, Copy)]
pub struct BehaviorTokenEconomyTerms {
    pub cost_override: f32,
    pub tracker: AssetId,
}

#[derive(Component)]
pub struct EducationDonationBonus;
