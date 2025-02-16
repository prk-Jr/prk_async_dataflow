use prometheus::{Encoder, TextEncoder, Registry};

lazy_static::lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
}

pub fn gather_metrics() -> String {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    let metric_families = REGISTRY.gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}