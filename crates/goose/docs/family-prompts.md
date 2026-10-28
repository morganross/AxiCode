# Family prompts

These prompts are AxiCode's. Upstream Goose `system.md` is unchanged, and it is the prompt a session uses while family prompts are off.

`GOOSE_FAMILY_PROMPTS` defaults to false. AxiOwl Desktop writes the same default: a missing `family-prompts.json` leaves the switch off. Turning the switch on does not require a Goose rebuild. The text is chosen from the session model name.

| Model name contains | File |
|---|---|
| `claude` | `src/prompts/family_anthropic.md` |
| `gpt` or `codex` | `src/prompts/family_gpt.md` |
| anything else | `src/prompts/family_default.md` |

`src/prompts/prompt_context.md` is rendered after the family file. The working rules live there once.

- Read the code a change depends on before editing it.
- Use `edit` for a unique replacement. Use `write` only for a new file or a full replacement.
- Do not rewrite source from the shell. Leave edits you did not make alone.
- Do not commit, push, amend, or run `git reset --hard` or `git checkout --` unless the user asked.
- Do not repeat the same failed tool call. Change the approach.
- `todo_write` is a short plan. Rewrite it only when the plan changes. Do not check items off.
- When a change can be checked, run the narrowest check that covers it.
- Send a wide search to a subagent. Search a known file or symbol in this session.
- Cite code as `path:line`. On a review, findings come first.

The family files are tone and answer shape only.

- Claude: short replies, no narration of routine reads, disagree when the code disagrees.
- GPT and Codex: finish the check before the answer, do not paste a file that was just written, flat lists, do not open with "Done".
- Every other model: concise and factual.

Search is not named in the family text. Outside code mode, the model is told to search the way the developer instructions describe for this machine. Those instructions say `findstr` or `Select-String` on Windows and `rg` elsewhere.

In code mode the script has no shell. The shared text tells the model to batch work inside `async function run()` and call the tools it discovered with `tools.call`.

`src/prompts/` is compiled into the Goose binary. This note stays outside that directory so it is not linked into `goose.exe`.
