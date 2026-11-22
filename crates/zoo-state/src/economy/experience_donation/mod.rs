use bevy::prelude::*;

use super::money_types::Money;

/// Amount and beneficiary carried by a guest's authored donation task token.
#[derive(Component)]
pub struct ExperienceDonation {
    pub subject: Entity,
    pub amount: Money,
    pub submitted: bool,
}
