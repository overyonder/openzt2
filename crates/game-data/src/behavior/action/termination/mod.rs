use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct BehaviorTerminationAction {
    pub kill_subject: bool,
    pub kill_target: bool,
}
