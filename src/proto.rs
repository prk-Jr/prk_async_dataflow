
#[cfg(feature = "protobuf")]
pub mod proto {
    use prost::Message;
    use serde_json::Value;
    use crate::JsonParserError;

    pub trait ProtoConverter: Message + Default {
        fn to_json(&self) -> Value;
        fn from_json(value: Value) -> Result<Self, JsonParserError>;
    }

    #[derive(Message)]
    pub struct GenericMessage {
        #[prost(map="string, bytes", tag="1")]
        pub fields: std::collections::HashMap<String, Vec<u8>>,
    }

    impl ProtoConverter for GenericMessage {
        fn to_json(&self) -> Value {
            let mut map = serde_json::Map::new();
            for (k, v) in &self.fields {
                map.insert(k.clone(), Value::String(base64::encode(v)));
            }
            Value::Object(map)
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