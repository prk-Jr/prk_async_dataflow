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

#[cfg(feature = "metrics")]
lazy_static::lazy_static! {
    pub static ref PROCESSING_TIME_HISTOGRAM: prometheus::Histogram = prometheus::register_histogram!(
        "processing_time_seconds",
        "Time taken to process messages",
        vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0]
    ).unwrap();
    
    pub static ref ERROR_COUNTER: prometheus::IntCounterVec = prometheus::register_int_counter_vec!(
        "error_count",
        "Number of errors by type",
        &["error_type"]
    ).unwrap();
}

#[cfg(feature = "metrics")]
pub fn record_error(error_type: &str) {
    ERROR_COUNTER.with_label_values(&[error_type]).inc();
}