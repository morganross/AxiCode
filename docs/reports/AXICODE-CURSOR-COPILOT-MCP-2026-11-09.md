# AxiCode Cursor and Copilot MCP delivery

November 9, 2026. This change concerns the Cursor and Copilot providers embedded in AxiCode.

The Cursor adapter discarded the configured session extensions and launched its independent tool loop without a plugin directory. Its provider is now session-bound and sends the resolved MCP definitions through a private, session-owned Cursor plugin. The browser and AxiAccess helpers can keep their existing capability and grant requirements. Native account selection, model, command path and stdin prompt handling keep their existing contracts.

On Linux, the installed Copilot 1.0.107 ACP client acknowledged a new session with a disposable stdio server but never started it. The same client started the server and completed initialize and tools/list when given its supported additional MCP config launch flag. There were no model prompts in this comparison. AxiCode now launches the resolved stdio definitions through that flag and keeps ACP delivery for the other transports. The private launch file lives through connection and session initialization.

The adapters share the existing stdio and HTTP serialization used by Claude Code. The Claude caller retains its existing configuration shape. Both new files containing capability material use the existing private-file writer, including its Windows owner-only handling. The changes are shared by the native Windows, Mac and Linux AxiCode builds. Live verification targets Linux for this request.

Actual pre-release verification included the Copilot ACP versus launch-flag probe, formatting the touched Rust files and a clean git diff check. The new Cursor recording-shim regression checks its exact plugin argument, mode and disposal when run. The IDE's follow-up report records the installed build and live pathway results.
