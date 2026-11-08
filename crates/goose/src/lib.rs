#![recursion_limit = "256"]

#[cfg(any(
    all(feature = "rustls-tls-common", feature = "native-tls"),
    all(feature = "rustls-tls", feature = "rustls-ring-tls")
))]
compile_error!("Select exactly one TLS backend");

#[cfg(all(
    feature = "rustls-tls-common",
    not(any(feature = "rustls-tls", feature = "rustls-ring-tls"))
))]
compile_error!("Select a concrete Rustls crypto provider");

#[cfg(any(feature = "rustls-tls", feature = "rustls-ring-tls"))]
pub fn install_rustls_provider() {
    #[cfg(feature = "rustls-tls")]
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    #[cfg(feature = "rustls-ring-tls")]
    let provider = rustls::crypto::ring::default_provider();
    let _ = provider.install_default();
}

pub mod acp;
pub use goose_sdk_types::{custom_notifications, custom_requests};
pub mod action_required_manager;
pub mod agents;
pub mod builtin_extension;
pub mod checks;
pub mod config;
pub mod context_limit;
pub mod context_mgmt;
pub mod conversation {
    pub use goose_providers::conversation::*;
}
pub mod dictation;
pub mod doctor;
pub mod download_manager;
pub mod elicitation;
pub mod execution;
pub mod gateway;
pub mod goose_apps;
pub mod hints;
pub mod hooks;
pub mod instance_id;
mod live_voice;
pub mod logging;
pub mod mcp_utils;
#[cfg(feature = "online-model-meta")]
pub mod model_catalog;
pub mod model_config;
pub mod oauth;
#[cfg(feature = "otel")]
pub mod otel;
pub mod permission;
pub mod plugins;
#[cfg(feature = "telemetry")]
pub mod posthog;
pub mod prompt_template;
pub mod providers;
pub mod recipe;
pub mod recipe_deeplink;
pub mod scheduler;
pub mod scheduler_trait;
pub mod security;
pub mod session;
pub mod session_context;
pub mod skills;
pub mod slash_commands;
pub mod source_roots;
pub mod sources;
pub mod subprocess;
pub mod token_counter;
mod tool_call_labels;
pub mod tool_inspection;
pub mod tool_monitor;
pub mod tracing;
pub mod utils;
