# Preserve Cursor model IDs in AxiCode

The Linux installed A4 pilot admitted `cursor-agent/gpt-5.6-luna-none` and stopped before the provider prompt. A separate no-prompt reproduction with the installed Goose ACP binary verified that both session creation and the explicit model setter returned `gpt-5.6-luna` for `gpt-5.6-luna-none`. In `model_config.rs`, generic effort-suffix normalization stripped the suffix. The Cursor adapter passes `model_name` verbatim to its CLI's `--model` option, so accepting the shortened readback would have permitted a different model option.

Cursor now retains its exact provider-owned model identifier in the shared model configuration. The provider-specific exception applies wherever the Cursor adapter is consumed on Windows, Linux and macOS. Other providers retain their existing normalization. The IDE's exact provider/model guard remains in place. The evidence for this change is the Linux A4 no-prompt ACP trace and source in `crates/goose/src/model_config.rs`, `crates/goose/src/acp/server.rs`, and `crates/goose/src/providers/cursor_agent.rs`. A focused regression checks exact Cursor IDs and unchanged OpenAI normalization. Build and installed verification are pending.

## Saved session readback

The first compiled producer `6cd09afa` still shortened the ID during the same no-prompt reproduction. The saved session reader deserializes `ModelConfig` and generic deserialization performs its own effort normalization. The existing provider-scoped exact decode for Azure Foundry now also handles Cursor, preserving both the original ID and its original request parameters. A regression checks the persisted Cursor variant. This follow-up is outside the first Linux build snapshot and needs a subsequent producer build.

Focused Linux verification: `cargo test --locked -p goose cursor_session_model_config_keeps_exact_model_id --lib` passed (1 test); the existing debug cache was preserved on the attached disk. This verifies persisted model identity in isolation, not a live Cursor response on any OS.
