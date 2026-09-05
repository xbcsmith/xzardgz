# Next Plans

**1. `cli_workflow_engine_integration_plan`** — do this first. `WorkflowExecutor` already works; this just connects the CLI to it. Nothing else is end-to-end testable or demoable until this lands, and `git_write_operations` and `watcher_xzepr_integration` both explicitly build their new stages on the `ExecutionInput`/stage pattern this plan establishes.

**2. `agent_tool_calling_integration_plan`** — do next, can overlap with #1 since it touches plugin internals, not CLI. This is a hard prerequisite for two other plans (`investigation_batching_plan` says it directly wires into "the plugin investigation path built in Plan 1"; `prompt_templating_system_plan`'s system-prompt override wires into the same `AgentSession` call).

With those two foundations in, the following can proceed **in parallel, in any order**:

**3. `confidence_scoring_integration_plan`** — no dependencies of its own, but it's a prerequisite for two later plans (OSV's Phase 4 and SAST's Phase 3 both explicitly feed into "the same `ScoringSignal`/`ConfidenceScorer` pipeline established" here), so don't leave it for last.

**4. `governance_agents_md_parsing_plan`** — fully standalone, nothing else depends on it. Good filler work whenever convenient.

**5. `investigation_batching_plan`** — right after #2 specifically, since it has a hard dependency on it.

**6. `prompt_templating_system_plan`** — Phase 1 (Tera loader) anytime; Phase 2 (system-prompt override) needs #2 done first.

Then, once #1 and #3 are in place:

**7. `external_data_clients_plan`** — Phase 1 (Scorecard/repodata) only needs #1; the OSV Phase 4 scoring work needs #3.

**8. `sast_scanning_tool_plan`** — Phases 1-2 (rule loader, AST engine) are fully standalone and could actually start on day one in parallel with everything above, since they don't touch existing plugin code at all. Phase 3 (wiring into `security_review`) needs #3, and benefits from #2 being done too.

**9. `git_write_operations_plan`** — Phase 1 (branch/commit/push primitives) is standalone and could also start early; Phase 2 (PR as an executor stage) needs #1.

**10. `watcher_xzepr_integration_plan`** — technically only needs #1, but I'd sequence it last on purpose: it's the production event-driven path, and there's no point wiring it live before the plugins it dispatches to (#2, #3, #5, #6) are actually good.

**Running throughout: `demo_directories_plan`** — this one isn't a step in the sequence, it's a companion track. Start Phase 1 immediately (the MCP demo works standalone today, no dependencies), then add one demo directory as each plan above lands, per its own design.
