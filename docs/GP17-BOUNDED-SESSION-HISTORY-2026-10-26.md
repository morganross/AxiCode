# GP17 bounded session history — 2026-10-26

## Evidenced cause and change

The existing ACP `on_export_session` delegates to `SessionManager::export_session`, returning the full serialized conversation. It provides neither a bounded page nor an exact persisted transcript revision, forcing consumers to obtain all messages or infer freshness from database-wide file changes. Current `get_conversation` orders native messages by `(created_timestamp,id)` and decodes only user/assistant roles. The new path preserves this ordering and native `Message` serialization.

Three additive read-only ACP methods use camelCase fields:

- `_goose/unstable/session/historyCapabilities` `{}` returns `{version:1,maxPageMessages:128,maxPageBytes:1048576}`.
- `_goose/unstable/session/historyRevision` `{sessionId}` returns `{version:1,sessionId,revision,totalMessages,rawUserCount}`. Counts and revision come from one SQLite read transaction. `rawUserCount` is deliberately not the UI canonical visible-text count.
- `_goose/unstable/session/historyPage` `{sessionId,revision,offset,limit}` returns `{version:1,sessionId,revision,messages,nextOffset}`. Offset is the zero-based raw user/assistant ordinal, limit is 1..128, and nextOffset is null at the end. A caller can request the tail using `max(totalMessages-limit,0)`.

Additive, idempotent schema migration 17 adds an independent revision row per session, initialized from SQLite randomblob(32). Message insert/update/delete triggers rotate the revision within the writer transaction, including replacement, compaction, metadata and timestamp edits. Session creation and deletion maintain revision lifetime, so a reused session id receives a new token. Unrelated sessions do not rotate this token. Each page validates the expected revision and reads its ordered range within one SQLite read transaction. A changed revision fails with RPC error `-32062` rather than mixing snapshots; consumers must reacquire the revision and restart.

Bounds are enforced before content is materialized: SQL reads at most 129 scalar size estimates; a conservative six-times string byte estimate plus 512 bytes per message accounts for JSON escaping and native fields, and 8192 bytes are reserved for the response identity/envelope used by ordinary numeric-id callers. Serialized message bytes are checked again before returning. Arbitrarily large caller-supplied JSON-RPC ids are outside the result budget. Session ids are limited to 1024 UTF-8 bytes, revisions to 64 characters. The entire requested range must fit; RPC error `-32061` means reduce the requested range or use the existing full export. A page never returns only the oldest prefix of a requested tail. Oversized rows are not truncated. Full export request and response types and implementation are unchanged.

## Compatibility and OS boundaries

The producer owns the SQLite schema and message decoding; IDE consumers need no private SQLite/schema access. Profile identity is the existing ACP process configuration/data directory, not a new cross-profile selector. Responses bind the selected session id and revision; callers must continue to bind their chosen profile to that process. No agent is acquired, provider called, tool started, or session activated by these handlers. Initializing storage may apply the producer's normal schema migration.

This is shared platform-neutral Rust/SQLite behavior consumed on Windows, Linux and macOS. There is no OS-specific cause or adapter change, package change, runtime redistribution, or credential change. Older clients continue using the same full-export method. Existing databases retain all messages; migration adds only a revision table and triggers. Runtime cross-OS behavior and downgrade compatibility have not been observed in this work.

## Verification

The four changed Rust files were formatted; `git diff --check` completed without whitespace diagnostics. Focused `cargo check -p goose --no-default-features --features rustls-tls,system-keyring` is compiling dependencies at source publication; compilation is explicitly unverified at publication and its eventual result will be recorded separately. The owner directed publication after review without adding cold compilation or clippy cycles as a release gate. The initial foreground check exceeded the SSH wrapper timeout while dependencies compiled and was interrupted; it is not a passing result. A subsequent command first used a nonexistent cargo/bin executable path and failed before compilation; the corrected command uses `/usr/bin/cargo` with the configured isolated CARGO_HOME/RUSTUP_HOME.

No conversation tests, paid prompts, live recipient attempts, release builds, or installations were run. No Mailbox evidence was used. Functional snapshot races, runtime migration, page byte accounting, end-to-end consumers, and Windows/macOS execution remain unverified until the owner-authorized real-conversation workflow runs.

Sources actually inspected: producer root `AGENTS.md`; `crates/goose-sdk-types/src/custom_requests.rs`; `crates/goose/src/acp/server/custom_dispatch.rs`; `crates/goose/src/acp/server/manage_sessions.rs`; `crates/goose/src/session/session_manager.rs`; `crates/goose/Cargo.toml`; IDE source-acquisition `shared/desktop/goose-source.json` and `scripts/shared/prepare-goose-source.cjs` (read only). No external issue or upstream design document was used.
