#[cfg(feature = "config")]
pub mod configuration {
    use serde::Deserialize;
    use config::Config;

    #[derive(Debug, Deserialize)]
    pub struct AgentConfig {
        pub name: String,
        pub rules: Vec<String>,
        pub parameters: simd_json::owned::Value,
    }

    #[derive(Debug, Deserialize)]
    pub struct PipelineConfig {
        pub input: String,
        pub output: String,
        pub agents: Vec<AgentConfig>,
    }

    pub fn load_config(path: &str) -> Result<PipelineConfig, config::ConfigError> {
        let settings = Config::builder()
            .add_source(config::File::with_name(path))
            .build()?;

        settings.try_deserialize()
    }
}