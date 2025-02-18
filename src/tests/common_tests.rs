#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        Agent, AgentAction, AgentSystem, AsyncJsonParser, FeatureTransformer, RuleBasedAgent,
    };

    // use tokio::sync::mpsc;
    use simd_json::{base::ValueAsScalar, derived::ValueObjectAccess, json, owned::Value};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_async_parser_basic() {
        let data = b"{\"a\":1}\n{\"a\":2}";
        let reader = tokio::io::BufReader::new(&data[..]);
        let mut parser = AsyncJsonParser::new(reader);

        let first: Value = parser.next().await.unwrap();
        assert_eq!(first["a"], 1);

        let second: Value = parser.next().await.unwrap();
        assert_eq!(second["a"], 2);
    }

    #[tokio::test]
    async fn test_feature_transformer() {
        let mut transformer = FeatureTransformer::new();
        transformer.add_mapping(
            "value".to_string(),
            Box::new(|v| {
                let num = v.as_i64().unwrap_or(0);
                json!({ "original": num, "doubled": num * 2 })
            }),
        );

        let input = json!({ "value": 5 });
        let output = transformer.transform(input.into());

        assert_eq!(output["value"]["doubled"].as_i64(), Some(10));
    }

    #[tokio::test]
async fn test_agent_system() {
    let (input_tx, input_rx) = mpsc::channel(10);
    let (output_tx, mut output_rx) = mpsc::channel(10);

    let transformer = Arc::new(FeatureTransformer::new());
    let agent = RuleBasedAgent::new(transformer.clone());
    agent.add_rule(
        "test_rule".into(),
        Box::new(|data: &Value| {
            if data.get("trigger").is_some() {
                AgentAction::ExternalCall("test".into(), json!({"status": "triggered"}))
            } else {
                AgentAction::Continue
            }
        }),
    );

    let agent_system = AgentSystem::new(input_rx, output_tx.clone());
    agent_system.lock().await.add_agent("test_agent", Arc::new(agent));

    let agent_system_clone = agent_system.clone();
    // tokio::spawn(async move {
    //     agent_system_clone.lock().await.run().await;
    // });
    agent_system_clone.lock().await.run().await;

    input_tx.send(json!({"trigger": true})).await.unwrap();
    let result = output_rx.recv().await.unwrap();
    assert_eq!(result["status"], "triggered");
}

    #[tokio::test]
    async fn test_rule_based_agent() {
        let transformer = Arc::new(FeatureTransformer::new());
        let agent = RuleBasedAgent::new(transformer);

        agent.add_rule(
            "test_rule".into(),
            Box::new(|data: &Value| {
                if data.get("trigger").is_some() {
                    AgentAction::ExternalCall("test".into(), json!({"status": "triggered"}))
                } else {
                    AgentAction::Continue
                }
            }),
        );

        let input_data = json!({"trigger": true});
        let (action, _) = agent.process(&input_data).await;

        assert!(matches!(action, AgentAction::ExternalCall(_, _)));
    }

    #[cfg(feature = "ml")]
    #[tokio::test]
    async fn test_ml_agent() {
        let model = Arc::new(TorchModel::new());
        let transformer = Arc::new(FeatureTransformer::new());
        let agent = MLAgent::new(model, transformer);

        let (action, _) = agent.process(&json!([1.0, 2.0, 3.0, 4.0, 5.0])).await;
        assert!(matches!(action, AgentAction::ExternalCall(_, _)));
    }

    // #[tokio::test]
    // async fn test_full_pipeline() {
    //     let (input_tx, input_rx) = mpsc::channel(10);
    //     let (output_tx, mut output_rx) = mpsc::channel(10);

    //     let mut transformer = FeatureTransformer::new();
    //     transformer.add_mapping("data".into(), Box::new(|v| (v.as_i64().unwrap_or(0) * 2).into()));
    //     let transformer = Arc::new(transformer);

    //     let agent_system = AgentSystem::new(input_rx, output_tx.clone());
    //     agent_system.add_agent("test_agent", Arc::new(RuleBasedAgent::new(transformer)));

    //     tokio::spawn(agent_system.run());

    //     let data = simd_json::to_vec(&json!({"data": 5})).unwrap();
    //     input_tx.send(data).await.unwrap();
    //     let result = output_rx.recv().await.unwrap();
    //     let result: Value = simd_json::from_slice(&mut result.clone()).unwrap();
    //     assert_eq!(result["data"], 10);
    // }
}
