use std::collections::HashMap;
use std::env;

use base64::Engine;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::ObservabilityConfig;

pub fn emit_check() {
    let span = tracing::info_span!(
        "observability.check",
        "langfuse.trace.name" = "observability.check",
        "langfuse.session.id" = "observability-check",
        "langfuse.environment" = "development",
        "langfuse.observation.type" = "span",
        "langfuse.observation.input" = "{\"source\":\"agent observability-check\"}",
        "langfuse.observation.output" = "{\"ok\":true}",
    );
    span.in_scope(|| tracing::info!("local Langfuse connectivity check"));
}

pub struct Telemetry {
    provider: Option<SdkTracerProvider>,
}

impl Telemetry {
    pub fn init(config: &ObservabilityConfig) -> Result<Self, String> {
        if !config.enabled {
            return Ok(Self { provider: None });
        }

        let endpoint = env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
            .unwrap_or_else(|_| config.endpoint.clone());
        let public_key = env::var("LANGFUSE_PUBLIC_KEY")
            .ok()
            .or_else(|| config.public_key.clone())
            .ok_or_else(|| {
                "observability is enabled but LANGFUSE_PUBLIC_KEY/observability.public_key is missing"
                    .to_string()
            })?;
        let secret_key = env::var("LANGFUSE_SECRET_KEY")
            .ok()
            .or_else(|| config.secret_key.clone())
            .ok_or_else(|| {
                "observability is enabled but LANGFUSE_SECRET_KEY/observability.secret_key is missing"
                    .to_string()
            })?;
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{public_key}:{secret_key}"));
        let headers = HashMap::from([
            ("Authorization".to_string(), format!("Basic {credentials}")),
            ("x-langfuse-ingestion-version".to_string(), "4".to_string()),
        ]);
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .with_protocol(Protocol::HttpBinary)
            .with_headers(headers)
            .build()
            .map_err(|error| format!("failed to create Langfuse OTLP exporter: {error}"))?;
        let provider = SdkTracerProvider::builder()
            .with_resource(
                Resource::builder()
                    .with_service_name("chenyanchen-agent")
                    .build(),
            )
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("agent-cli");

        tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()
            .map_err(|error| format!("failed to initialize OpenTelemetry: {error}"))?;

        Ok(Self {
            provider: Some(provider),
        })
    }

    pub fn shutdown(self) {
        if let Some(provider) = self.provider
            && let Err(error) = provider.shutdown()
        {
            eprintln!("failed to flush OpenTelemetry traces: {error}");
        }
    }
}
