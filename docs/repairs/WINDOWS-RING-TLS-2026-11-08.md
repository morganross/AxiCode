# Ring crypto option for the Windows IDE runtime

The Windows Goose 1.56.0 image reproduced a fatal abort during local TLS initialization. The captured stack reached AWS-LC 0.45.0's one-time random-number initializer after its Jitter Entropy collector returned null. The reproduction used the original signed image and a local HTTPS fixture. The original live A6 failures had the same executable and fault location, with no captured stacks.

The new `rustls-ring-tls` feature keeps Rustls and selects `ring` through reqwest, RMCP, axum-server and rcgen. The CLI installs its crypto provider before it opens any connections. Provider TLS configuration and ACP certificate fingerprints keep their existing formats. The other Rustls feature and the native TLS feature remain available for existing consumers. The desktop release selects the new feature on Windows.

Cargo 1.99.0 resolved the locked normal and build dependency graph on the Windows node. The new Windows selection resolved `ring` 0.17.14 and no AWS-LC packages. The Linux x64 and macOS ARM64 package sets match their previous selections. Changed Rust files were formatted and `git diff --check` passed. Native compilation and runtime verification are pending.

Evidence comes from the Windows IDE development checkout's `output/windows-a6-cold-tls-20261108T230559Z` and `output/goose-ring-graph-20261108T232435Z`, plus the AxiCode manifests and the node's cached dependency source. [Rustls crypto providers](https://docs.rs/rustls/0.23.46/rustls/crypto/struct.CryptoProvider.html)
