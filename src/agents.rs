use async_trait::async_trait;
use dashmap::DashMap;
use simd_json::owned::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::FeatureTransformer;

#[derive(Debug)]
pub enum AgentAction {
    Continue,
    Stop,
    Branch(Vec<String>),
    ExternalCall(String, Value),
}

#[async_trait]
pub trait Agent: Send + Sync {
    async fn process(&self, data: &Value) -> (AgentAction, Value);
    async fn learn(&self, experience: &Value);
}

#[cfg_attr(feature = "ml", async_trait)]
pub trait Model: Send + Sync {
    fn predict(&self, input: &Value) -> impl std::future::Future<Output = Value> + Send;
    fn train(&self, experience: &Value) -> impl std::future::Future<Output = ()> + Send;
}

#[cfg(feature = "ml")]
pub struct TorchModel {
    model: tch::CModule,
}

#[cfg(feature = "ml")]
impl TorchModel {
    pub fn new(cmodule_path: &str) -> Self {
        let model = tch::CModule::load(cmodule_path).unwrap_or_else(|_| {
            let mut model = tch::CModule::create(&[5]).unwrap();
            model.add_fc(5, 2).unwrap();
            model
        });
        Self { model }
    }
}

#[cfg(feature = "ml")]
#[async_trait]
impl Model for TorchModel {
    async fn predict(&self, input: &Value) -> Value {
        let features = input.as_array().map(|arr| 
            arr.iter()
               .filter_map(|v| v.as_f64())
               .collect::<Vec<f64>>()
        ).unwrap_or_default();
        
        let tensor = tch::Tensor::of_slice(&features)
            .to_kind(tch::Kind::Float)
            .unsqueeze(0);
        
        let output = self.model.forward_ts(&[tensor]).unwrap();
        let probabilities = output.softmax(-1, tch::Kind::Float);
        
        serde_json::json!({
            "output": probabilities.into(),
            "decision": probabilities.argmax(-1, false).int64_value(&[0])
        })
    }

    async fn train(&self, _experience: &Value) {
        // Training logic
    }
}

pub struct RuleBasedAgent {
    rules: DashMap<String, Box<dyn Fn(&Value) -> AgentAction + Send + Sync>>,
    transformer: Arc<FeatureTransformer>,
}

impl RuleBasedAgent {
    pub fn new(transformer: Arc<FeatureTransformer>) -> Self {
        Self {
            rules: DashMap::new(),
            transformer,
        }
    }

    pub fn add_rule<F>(&self, name: String, rule: F)
    where
        F: Fn(&Value) -> AgentAction + 'static + Send + Sync,
    {
        self.rules.insert(name, Box::new(rule));
    }
}

#[async_trait]
impl Agent for RuleBasedAgent {
    async fn process(&self, data: &Value) -> (AgentAction, Value) {
        let transformed = self.transformer.transform(data.clone());
        for rule in self.rules.iter() {
            let action = (rule.value())(&transformed);
            if !matches!(action, AgentAction::Continue) {
                return (action, transformed);
            }
        }
        (AgentAction::Continue, transformed)
    }

    async fn learn(&self, _experience: &Value) {
        // Rule-based agents typically don't learn
    }
}

#[cfg(feature = "ml")]
pub struct MLAgent {
    model: Arc<dyn Model>,
    transformer: Arc<FeatureTransformer>,
}

#[cfg(feature = "ml")]
impl MLAgent {
    pub fn new(model: Arc<dyn Model>, transformer: Arc<FeatureTransformer>) -> Self {
        Self { model, transformer }
    }
}

#[cfg(feature = "ml")]
#[async_trait]
impl Agent for MLAgent {
    async fn process(&self, data: &Value) -> (AgentAction, Value) {
        let transformed = self.transformer.transform(data.clone());
        let prediction = self.model.predict(&transformed).await;
        (AgentAction::ExternalCall("model_api".into(), prediction), transformed)
    }

    async fn learn(&self, experience: &Value) {
        self.model.train(experience).await;
    }
}

pub struct AgentSystem {
    agents: DashMap<String, Arc<dyn Agent + Send + Sync + 'static>>,
    input_rx: mpsc::Receiver<Value>,
    output_tx: mpsc::Sender<Value>,
}

impl AgentSystem {
    pub fn new(input_rx: mpsc::Receiver<Value>, output_tx: mpsc::Sender<Value>) -> Self {
        Self {
            agents: DashMap::new(),
            input_rx,
            output_tx,
        }
    }

    pub fn add_agent(&mut self, name: &str, agent: Arc<dyn Agent + Send + Sync + 'static>) {
        self.agents.insert(name.to_string(), agent);
    }

    pub async fn run(mut self) {
        while let Some(data) = self.input_rx.recv().await {
            for agent in self.agents.iter() {
                let (action, transformed) = agent.value().process(&data).await;
                self.handle_action(action, transformed).await;
            }
        }
    }

    async fn handle_action(&self, action: AgentAction, data: Value) {
        match action {
            AgentAction::Continue => {}
            AgentAction::Stop => {}
            AgentAction::Branch(agents) => {
                for agent_name in agents {
                    if let Some(agent) = self.agents.get(&agent_name) {
                        let (new_action, new_data) = agent.process(&data).await;
                        let fut = self.handle_action(new_action, new_data);
                        Box::pin(fut).await;
                    }
                }
            }
            AgentAction::ExternalCall(_, data) => {
                let _ = self.output_tx.send(data).await;
            }
        }
    }
}