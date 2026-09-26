use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorFeedbackAction {
    pub entries: Vec<BehaviorFeedbackEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorFeedbackEntry {
    Action {
        localization_key: String,
        use_entity_name: bool,
        use_target_name: bool,
    },
    Thought {
        localization_key: String,
        priority: Option<i16>,
        timeout_seconds: Option<u16>,
        globally_visible: Option<bool>,
        use_entity_name: bool,
        use_target_name: bool,
    },
    Message {
        localization_key: String,
        priority: Option<i16>,
        timeout_seconds: Option<u16>,
        interval_seconds: Option<u16>,
        tolerance: Option<f32>,
        globally_visible: Option<bool>,
        use_entity_name: bool,
        use_target_name: bool,
        filter_attribute_expression: Option<String>,
    },
    Emoticon {
        emoticon_name: String,
    },
}
