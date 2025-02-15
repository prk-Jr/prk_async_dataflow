// use serde_json::Value;
use simd_json::derived::MutableObject;
use std::collections::HashMap;
use simd_json::borrowed::Value;

pub struct FeatureTransformer {
    pub mappings: HashMap<String, Box<dyn Fn(Value) -> Value + Send + Sync>>,
}

impl FeatureTransformer {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, key: String, transform: Box<dyn Fn(Value) -> Value + Send + Sync>) {
        self.mappings.insert(key, transform);
    }

    pub fn transform<'a>(&self, data: Value<'a>) -> Value<'a> {
        let mut result = data.clone();
        for (key, transform) in &self.mappings {
            if let Some(value) = result.get_mut(key.as_str()) {
                *value = transform(value.clone());
            }
        }
        result
    }
}