
#[cfg(test)]
mod test {
    use simd_json::{base::ValueAsScalar, derived::ValueObjectAccess, borrowed::Value};

    use crate::FeatureTransformer;
    #[test]
fn test_title_uppercase_transformation() {
    // Create a JSON object that contains a title field.
    let json_str = r#"{
        "id": 1,
        "title": "hello world",
        "body": "example text"
    }"#;
    
    // Parse the JSON string into a simd_json Value.
    let mut json_bytes = json_str.as_bytes().to_vec();
    let  value: Value = simd_json::to_borrowed_value(&mut json_bytes).unwrap();
    
    // Set up the transformer.
    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|v| {
        if let Some(s) = v.as_str() {
            Value::String(s.to_uppercase().into()).into()
        } else {
            v
        }
    }));
    
    // Apply the transformation.
    let transformed = transformer.transform(value.clone().into());
    
    // Verify that the title field is now uppercase.
    assert_eq!(
        transformed.get("title").unwrap().as_str().unwrap(),
        "HELLO WORLD"
    );
    
    // Verify other fields remain unchanged.
    assert_eq!(
        transformed.get("body").unwrap().as_str().unwrap(),
        "example text"
    );
}

}