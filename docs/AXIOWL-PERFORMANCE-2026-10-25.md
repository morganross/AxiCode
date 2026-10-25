# AxiCode performance implementation — October 25, 2026

Owner-authorized component optimizations; no builds. Base `0e31c19d9b77b6637a300a74c6e3bcf21a1a2955`. IDE audit CP-044–047/053 is the source record.

- Read tool: stream selected line output with one reusable line buffer while validating the full UTF-8 input, retaining newline/range/error behavior. Unrestricted reads move their String rather than copy it.
- Tree line counts: reusable buffered validation/counting rather than whole-file allocations; invalid files still count zero, ignore/depth/output contracts unchanged.
- Edit errors: exact match count with only first two offsets, bounded preview/context allocation; ambiguous matches still rejected.
- Shared developer tool dispatcher: read/write/edit/tree use a two-slot blocking worker lane. Both legacy and state-machine paths use this dispatcher. Cancellation before admission is honored; an already running filesystem operation retains its existing synchronous completion behavior.
- Summarization: eligibility/candidate checks borrow history; owning history clone occurs only for an actual spawned task. Only the legacy loop calls this helper; the state-machine loop has no matching helper call, so no loop behavior is changed there.

Shared Rust on Windows/Linux/Mac; no provider/model choice, chat storage, credentials, tool permissions or recovery policy changed. Verification is source inspection/Rust formatting only; compiler/runtime/GUI results remain unverified. The owner's no-build instruction supersedes upstream build/clippy workflow suggestions for this task.

- Images: bounded two-job decode/crop/encode workers, local read offload, shared HTTP transport, and early release of original decoded bitmap before base64/PNG encoding. Full decoding/validation, exact original bytes, crop, supported formats and byte limits retained. No lower-resolution substitution or new pixel rejection.
