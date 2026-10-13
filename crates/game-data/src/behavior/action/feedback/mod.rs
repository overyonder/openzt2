use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorFeedbackAction {
    pub entries: Vec<BehaviorFeedbackEntry>,
}

/// Authored feedback text and icons. The `use_*` flags choose which names
/// and contents substitute into the localized text.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorFeedbackEntry {
    Action {
        localization_key: String,
        use_entity_name: bool,
        use_target_name: bool,
        /// The target container's contents, such as the food in a dish.
        use_target_contents: bool,
        use_object_name: bool,
    },
    Thought {
        localization_key: String,
        priority: Option<i16>,
        timeout_seconds: Option<u16>,
        globally_visible: Option<bool>,
        use_entity_name: bool,
        use_target_name: bool,
        /// The target's own task target, such as the dish an animal eats from.
        use_target_target: bool,
        use_target_object: bool,
        use_target_species: bool,
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
        emoticon_scale: Option<f32>,
        /// The model node the emoticon appears above, when authored.
        emoticon_entity_node: Option<String>,
    },
}
