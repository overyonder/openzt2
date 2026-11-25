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

/// Coordinates carried by a trick token, in world units.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BehaviorTrickDestination(pub Vec3);

/// Placement position to which a show prop's authored offsets are relative.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BehaviorTrickPropOrigin(pub Vec3);

/// Qualifier values carried by authored token payloads.
#[derive(Component, Default)]
pub struct BehaviorTokenFacts(
    pub  Vec<(
        openzt2_game_data::behavior::eligibility::BehaviorEligibilityFactInput,
        openzt2_game_data::behavior::eligibility::BehaviorEligibilityFactValue,
    )>,
);

impl BehaviorTokenFacts {
    pub fn satisfies(
        &self,
        requirement: &openzt2_game_data::behavior::eligibility::BehaviorCandidateEligibilityFact,
    ) -> bool {
        use openzt2_game_data::behavior::eligibility::{
            BehaviorEligibilityFactComparison as Comparison, BehaviorEligibilityFactValue as Value,
        };
        let Some((_, value)) = self
            .0
            .iter()
            .rev()
            .find(|(key, _)| *key == requirement.fact_input)
        else {
            return false;
        };
        match (*value, requirement.expected_value) {
            (Value::Q16(left), Value::Q16(right)) => match requirement.comparison {
                Comparison::Equal => left == right,
                Comparison::NotEqual => left != right,
                Comparison::Less => left < right,
                Comparison::LessOrEqual => left <= right,
                Comparison::Greater => left > right,
                Comparison::GreaterOrEqual => left >= right,
            },
            (Value::Symbol(left), Value::Symbol(right)) => match requirement.comparison {
                Comparison::Equal => left == right,
                Comparison::NotEqual => left != right,
                _ => false,
            },
            (
                Value::SymbolThreshold { symbol: left, q16 },
                Value::SymbolThreshold {
                    symbol: right,
                    q16: threshold,
                },
            ) => left == right && q16 >= threshold,
            _ => false,
        }
    }
}

/// An authored transaction's token-carried price and accounting category.
#[derive(Component, Clone, Copy)]
pub struct BehaviorTokenEconomyTerms {
    pub cost_override: f32,
    pub tracker: AssetId,
}

#[derive(Component)]
pub struct EducationDonationBonus;
