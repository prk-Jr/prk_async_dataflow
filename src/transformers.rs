// use serde_json::Value;
use simd_json::{derived::MutableObject, owned::Value};
use std::collections::HashMap;

pub struct FeatureTransformer {
    pub mappings: HashMap<String, Box<dyn Fn(Value) -> Value>>,
}

impl FeatureTransformer {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, key: String, transform: Box<dyn Fn(Value) -> Value>) {
        self.mappings.insert(key, transform);
    }

    pub fn transform(&self, data: Value) -> Value {
        let mut result = data.clone();
        for (key, transform) in &self.mappings {
            if let Some(value) = result.get_mut(key) {
                *value = transform(value.clone());
            }
        }
        result
    }
}