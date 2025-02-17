use simd_json::{derived::MutableObject,  OwnedValue};
use std::collections::HashMap;

pub struct FeatureTransformer {
    mappings: HashMap<String, Box<dyn Fn(OwnedValue) -> OwnedValue + Send + Sync>>,
}

impl FeatureTransformer {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(
        &mut self,
        key: String,
        transform: Box<dyn Fn(OwnedValue) -> OwnedValue + Send + Sync>,
    ) {
        self.mappings.insert(key, transform);
    }

    pub fn transform<'a>(&self, mut data: OwnedValue) -> OwnedValue {
        for (key, transform) in &self.mappings {
            if let Some(value) = data.get_mut(key.as_str()) {
                let new_value = transform(value.clone());
                *value = new_value;
            }
        }
        data
    }
}