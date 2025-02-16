use simd_json::{derived::MutableObject,  BorrowedValue};
use std::collections::HashMap;

pub struct FeatureTransformer {
    mappings: HashMap<String, Box<dyn Fn(BorrowedValue) -> BorrowedValue + Send + Sync>>,
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
        transform: Box<dyn Fn(BorrowedValue) -> BorrowedValue + Send + Sync>,
    ) {
        self.mappings.insert(key, transform);
    }

    pub fn transform<'a>(&self, mut data: BorrowedValue<'a>) -> BorrowedValue<'a> {
        for (key, transform) in &self.mappings {
            if let Some(value) = data.get_mut(key.as_str()) {
                let new_value = transform(value.clone());
                *value = new_value;
            }
        }
        data
    }
}