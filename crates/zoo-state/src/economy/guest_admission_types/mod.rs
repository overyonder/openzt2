use bevy::prelude::*;

use super::money_types::Money;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdmissionPrice {
    pub adult: Money,
    pub child: Money,
}

impl AdmissionPrice {
    /// A changed admission band sets the child fee to half the adult fee.
    pub fn from_adult(adult: Money) -> Self {
        let child = adult
            .checked_scale(0.5)
            .expect("halving a finite price is finite");
        Self { adult, child }
    }
}

/// Whether ordinary gate admissions are currently open.
///
/// This is authoritative zoo policy shared by the management UI and the guest
/// arrival producer. Scenario-authored forced arrivals remain separate typed
/// requests and do not rewrite this policy.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooAdmissionsOpen(pub bool);

/// Correlates one gate arrival with its authoritative admission transfer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = PendingGuestAdmission)]
pub struct AdmissionPaymentPending {
    pub guest: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = AdmissionPaymentPending, linked_spawn)]
pub struct PendingGuestAdmission(Entity);
