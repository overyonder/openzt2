use bevy::prelude::*;

/// One message in the zoo message bar, already localized.
#[derive(Debug, Clone, PartialEq)]
pub struct ZooMessage {
    pub text: String,
    /// Real time at which the message leaves the single-message bar.
    pub expires_at_seconds: f64,
}

/// Messages posted to the zoo message bar, oldest first. The bar shows the
/// newest unexpired one; the opened queue lists them all.
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct ZooMessages {
    pub messages: Vec<ZooMessage>,
}
