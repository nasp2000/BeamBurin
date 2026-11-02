//! Reference recipes: one operation setting for a material, a machine and a head.
//!
//! A recipe is a starting point, not a promise: the same material can behave
//! differently on another machine or with another lens. The library keeps the
//! numbers with the combination they were tested on so they can be found again.

use serde::{Deserialize, Serialize};

use crate::operations::Operation;
use crate::profiles::OperationProfile;

/// Operation settings bound to a material, a machine and a head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    /// Material profile id.
    pub material_id: String,
    /// Machine profile id; empty means any machine.
    #[serde(default)]
    pub machine_id: String,
    /// Laser head, lens or module label, for example "40 W diode".
    #[serde(default)]
    pub head: String,
    /// The settings to apply.
    pub operation: OperationProfile,
    #[serde(default)]
    pub notes: String,
}

impl Recipe {
    pub fn new(
        name: impl Into<String>,
        material_id: impl Into<String>,
        operation: OperationProfile,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            material_id: material_id.into(),
            machine_id: String::new(),
            head: String::new(),
            operation,
            notes: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("recipe id must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("recipe name must not be empty".to_string());
        }
        if self.material_id.trim().is_empty() {
            return Err("a recipe needs a material".to_string());
        }
        self.operation
            .validate()
            .map_err(|error| format!("recipe '{}': {error}", self.name))?;
        if self.machine_id == self.operation.id {
            return Err("a recipe cannot name a machine by operation id".to_string());
        }
        Ok(())
    }

    /// Copy the stored settings into an operation.
    pub fn apply_to(&self, operation: &mut Operation) {
        operation.op_type = self.operation.op_type;
        operation.params = self.operation.params.clone();
    }

    /// One line with the values that matter.
    pub fn summary(&self) -> String {
        format!(
            "{} - {:.0} mm/min, {:.0}%, {} pass(es), {:.2} mm",
            self.operation.op_type.label(),
            self.operation.params.speed,
            self.operation.params.power,
            self.operation.params.passes,
            self.operation.params.line_interval_mm
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Recipe;
    use crate::operations::{Operation, OperationParams, OperationType};
    use crate::profiles::OperationProfile;

    fn operation_profile() -> OperationProfile {
        let mut profile = OperationProfile::new("Diode plywood", OperationType::OutlineCutting);
        profile.params.speed = 900.0;
        profile.params.power = 65.0;
        profile.params.passes = 3;
        profile
    }

    #[test]
    fn a_recipe_validates_and_applies_its_settings() {
        let recipe = Recipe::new("Plywood 3 mm", "plywood-3", operation_profile());
        assert!(recipe.validate().is_ok());
        let mut operation = Operation::new("Cut", OperationType::Line);
        operation.params = OperationParams::default();
        recipe.apply_to(&mut operation);
        assert_eq!(operation.op_type, OperationType::OutlineCutting);
        assert!((operation.params.speed - 900.0).abs() < 1e-9);
        assert!((operation.params.power - 65.0).abs() < 1e-9);
        assert_eq!(operation.params.passes, 3);
        assert!(recipe.summary().contains("900 mm/min"));
    }

    #[test]
    fn a_recipe_needs_a_name_and_material() {
        let mut recipe = Recipe::new("", "plywood-3", operation_profile());
        assert!(recipe.validate().is_err());
        recipe.name = "Plywood".to_string();
        recipe.material_id.clear();
        assert!(recipe.validate().is_err());
        recipe.material_id = "plywood-3".to_string();
        recipe.operation.params.speed = 0.0;
        assert!(recipe.validate().is_err());
    }

    #[test]
    fn recipes_round_trip() {
        let recipe = Recipe::new("Plywood 3 mm", "plywood-3", operation_profile());
        let json = serde_json::to_string(&recipe).unwrap();
        let loaded: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, recipe);
        // The machine and the head are optional in older files.
        let old = serde_json::json!({
            "id": "r1",
            "name": "Old",
            "material_id": "plywood-3",
            "operation": recipe.operation,
        });
        let loaded: Recipe = serde_json::from_value(old).unwrap();
        assert_eq!(loaded.machine_id, "");
        assert_eq!(loaded.head, "");
    }
}
