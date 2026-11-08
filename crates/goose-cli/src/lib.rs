#![recursion_limit = "256"]

#[cfg(not(any(
    feature = "rustls-tls",
    feature = "rustls-ring-tls",
    feature = "native-tls"
)))]
compile_error!("At least one of `rustls-tls` or `native-tls` features must be enabled");

#[cfg(any(
    all(
        any(feature = "rustls-tls", feature = "rustls-ring-tls"),
        feature = "native-tls"
    ),
    all(feature = "rustls-tls", feature = "rustls-ring-tls")
))]
compile_error!("Select exactly one TLS backend");

pub mod cli;
pub mod commands;
pub mod logging;
pub mod recipes;
pub mod session;
pub mod signal;

// Re-export commonly used types
pub use cli::Cli;
pub use session::CliSession;
