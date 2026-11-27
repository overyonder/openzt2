use bevy::prelude::*;

/// A model was replaced or restored while the game runs, such as one sent
/// from Blender. Objects using it rebuild their render parts once it has
/// reloaded. Ordinary reloads, such as a model coming back into view, do not
/// send this.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PrefabModelReplaced {
    /// The model's asset path, without a label.
    pub model_path: String,
}

/// Models announced as replaced whose reload has not yet been seen.
#[derive(Default)]
pub struct PendingPrefabModelReplacements(Vec<String>);

impl PendingPrefabModelReplacements {
    pub fn record(&mut self, replaced: impl IntoIterator<Item = String>) {
        for model_path in replaced {
            let model_path = model_path.to_ascii_lowercase();
            if !self.0.contains(&model_path) {
                self.0.push(model_path);
            }
        }
    }

    /// Takes a reloaded prefab, named by its asset path, if it is a
    /// replaced model.
    pub fn take(&mut self, prefab_path: Option<&bevy::asset::AssetPath<'_>>) -> bool {
        let Some(path) = prefab_path else {
            return false;
        };
        let path = path.path().to_string_lossy().to_ascii_lowercase();
        let before = self.0.len();
        self.0.retain(|model_path| *model_path != path);
        self.0.len() != before
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
