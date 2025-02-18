
#[cfg(feature = "protobuf")]
pub mod proto {

    use prost::Message;
    use simd_json::{base::{ValueAsContainer, ValueAsScalar}, owned::Value};
    use crate::JsonParserError;

    pub trait ProtoConverter: Message + Default {
        fn to_json(&self) -> Value;
        fn from_json(value: Value) -> Result<Self, JsonParserError>;
    }

    #[derive(Message, Clone)]
    pub struct GenericMessage {
        #[prost(map="string, bytes", tag="1")]
        pub fields: std::collections::HashMap<String, Vec<u8>>,
    }

    impl ProtoConverter for GenericMessage {
        fn to_json(&self) -> Value {
            let mut map = halfbrown::HashMap::new();
            for (k, v) in &self.fields {
                map.insert(k.clone(), Value::String(base64::encode(v)));
            }
            let mut sized_map = halfbrown::SizedHashMap::with_capacity(map.len());
            for (k, v) in map {
                sized_map.insert(k, v);
            }
            Value::Object(Box::new(sized_map))
        }

        fn from_json(value: Value) -> Result<Self, JsonParserError> {
            let mut fields = std::collections::HashMap::new();
            let obj = value.as_object().ok_or(JsonParserError::InvalidData("Not an object".into()))?;
            for (k, v) in obj {
                let bytes = base64::decode(v.as_str().unwrap_or(""))
                    .map_err(|e| JsonParserError::InvalidData(e.to_string()))?;
                fields.insert(k.clone(), bytes);
            }
            Ok(Self { fields })
        }
    }
}