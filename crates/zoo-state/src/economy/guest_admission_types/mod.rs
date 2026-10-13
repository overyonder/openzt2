use bevy::prelude::*;

use super::money_types::Money;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdmissionPrice(pub Money);

/// Whether ordinary gate admissions are currently open.
///
/// This is authoritative zoo policy shared by the management UI and the guest
/// arrival producer. Scenario-authored forced arrivals remain separate typed
/// requests and do not rewrite this policy.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooAdmissionsOpen(pub bool);

/// Correlates one gate arrival with its authoritative admission transfer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionPaymentPending {
    pub guest: Entity,
}
