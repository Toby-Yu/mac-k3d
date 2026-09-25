# Question coverage on this worker

Which questions have a Harbor run on this PC, and the Docker memory recorded for each. Product packing rules stay in [optimization.md](../optimization.md). Branch notes: [eval-comparison-report](../eval-comparison-report/README.md).

Jenkins build numbers are the `jenkins-<n>` trees under each job’s `eval-runs/harness/harbor_runs`. `peak_gb`, `slots`, and `memory_mb` come from that run’s `container_mem.jsonl` after P8 (copied next to `artifact.json`). Historical builds finished before that file existed, so those cells stay empty.

A skip (`skip question=` / `skipped_questions.txt`) stays **not run**. An out-of-memory skip can still have a sampled `peak_gb` in that run’s `container_mem.jsonl`. After a finished P8, fill the three memory cells and set status to `run`.

## Next DeepSWE TASKS

Leave **TASK** empty. **N_ROLLOUTS=4**, **CPU_LOCK_QTY=4**. `TASKS` replaces `N_TASKS`.

```text
eicrud-keyset-pagination-cursor,scc-bounded-memory-spilling,meriyah-explicit-resource-declarations
```

These three are not in jenkins-21 through jenkins-30. Do not rebuild or run `mac-k3d eval --sync-pipeline` while that job is in P5.

## DeepSWE

5 run, 108 not run, 113 task dirs.

| question | status | builds | peak_gb | slots | memory_mb |
|----------|--------|--------|---------|-------|-----------|
| `abs-module-cache-flags` | run | 21, 22, 23, 24, 25, 27, 28, 29, 30 | | | |
| `abs-stepped-slices` | run | 23, 24, 25, 28, 29, 30 | | | |
| `actionlint-action-pinning-lint` | run | 23, 24, 25, 28, 29, 30 | | | |
| `adaptix-name-mapping-aliases` | run | 23, 24, 29, 30 | | | |
| `aiomonitor-task-snapshots-diff` | run | 23 | | | |
| `anko-default-function-arguments` | not run |  | | | |
| `anko-typed-variable-bindings` | not run |  | | | |
| `arcane-drift-detection-baselines` | not run |  | | | |
| `arktype-json-schema-refs-dependencies` | not run |  | | | |
| `awilix-async-container-initialization` | not run |  | | | |
| `bandit-incremental-cache-control` | not run |  | | | |
| `bandit-interprocedural-taint-checks` | not run |  | | | |
| `bandit-structured-nosec-directives` | not run |  | | | |
| `boa-hierarchical-evaluation-cancellation` | not run |  | | | |
| `cattrs-partial-structuring-recovery` | not run |  | | | |
| `clack-async-autocomplete-options` | not run |  | | | |
| `claude-code-by-agents-recursive-delegation` | not run |  | | | |
| `cliffy-config-file-parsing` | not run |  | | | |
| `csstree-shorthand-expansion-compression` | not run |  | | | |
| `dasel-html-document-format` | not run |  | | | |
| `dateutil-rfc5545-timezone-interop` | not run |  | | | |
| `drizzle-orm-window-function-builders` | not run |  | | | |
| `dynamodb-toolbox-conditional-attribute-requirements` | not run |  | | | |
| `dynamodb-toolbox-lazy-recursive-schemas` | not run |  | | | |
| `effect-sse-httpapi-streaming` | not run |  | | | |
| `eicrud-keyset-pagination-cursor` | not run |  | | | |
| `etree-xml-diff-patch` | not run |  | | | |
| `expr-try-catch-errors` | not run |  | | | |
| `fastapi-deprecation-response-headers` | not run |  | | | |
| `fastapi-implicit-head-options` | not run |  | | | |
| `fd-deterministic-multi-key-sorting` | not run |  | | | |
| `geo-shapeindex-serialization` | not run |  | | | |
| `go-critic-doc-link-checker` | not run |  | | | |
| `go-genai-streamed-function-args` | not run |  | | | |
| `go-git-worktree-merge-conflicts` | not run |  | | | |
| `goreleaser-retry-publish-auditing` | not run |  | | | |
| `gql-incremental-graphql-delivery` | not run |  | | | |
| `happy-dom-abort-pending-body-reads` | not run |  | | | |
| `happy-dom-deterministic-intersectionobserver` | not run |  | | | |
| `helm-array-merge-strategies` | not run |  | | | |
| `helm-unified-manifest-stream` | not run |  | | | |
| `httpx-deterministic-cookie-store` | not run |  | | | |
| `httpx-multipart-response-parsing` | not run |  | | | |
| `httpx-streaming-json-iteration` | not run |  | | | |
| `igel-persist-feature-schema` | not run |  | | | |
| `ink-grid-box-layout` | not run |  | | | |
| `ipython-session-bundle-replay` | not run |  | | | |
| `katex-multicolumn-array-spans` | not run |  | | | |
| `kcp-go-multiplexed-kcp-streams` | not run |  | | | |
| `kea-atomic-signal-selectors` | not run |  | | | |
| `kgateway-consistent-hash-policy` | not run |  | | | |
| `kombu-single-active-consumer-priority` | not run |  | | | |
| `kombu-virtual-queue-dead-lettering` | not run |  | | | |
| `koota-composite-trait-aspects` | not run |  | | | |
| `koota-deferred-mutation-buffer` | not run |  | | | |
| `koota-entity-snapshot-rollback` | not run |  | | | |
| `koota-pair-relation-tracking` | not run |  | | | |
| `koota-query-predicates` | not run |  | | | |
| `kysely-window-grouping-helpers` | not run |  | | | |
| `langchain-request-coalescing` | not run |  | | | |
| `mashumaro-flattened-dataclass-fields` | not run |  | | | |
| `meriyah-explicit-resource-declarations` | not run |  | | | |
| `mnamer-daemon-watch-lifecycle` | not run |  | | | |
| `mobly-grouped-test-barriers` | not run |  | | | |
| `narwhals-rolling-window-suite` | not run |  | | | |
| `numba-stencil-boundary-modes` | not run |  | | | |
| `obsidian-linter-auto-table-of-contents` | not run |  | | | |
| `obsidian-linter-link-format-conversion` | not run |  | | | |
| `obsidian-linter-scoped-ignore-markers` | not run |  | | | |
| `ofetch-per-origin-circuit-breaker` | not run |  | | | |
| `onedump-dump-encryption-pipeline` | not run |  | | | |
| `opa-rego-rule-profiling` | not run |  | | | |
| `opa-template-string-reconstruction` | not run |  | | | |
| `optique-conditional-option-dependencies` | not run |  | | | |
| `oxvg-structural-selector-preservation` | not run |  | | | |
| `participle-grammar-conflict-analysis` | not run |  | | | |
| `pebble-durability-wait-apis` | not run |  | | | |
| `pest-character-class-coalescing` | not run |  | | | |
| `prometheus-transactional-reload-status` | not run |  | | | |
| `prometheus-typed-label-sorting` | not run |  | | | |
| `psd-tools-blend-range-api` | not run |  | | | |
| `pwntools-tube-multiplexing` | not run |  | | | |
| `python-statemachine-state-data-scoping` | not run |  | | | |
| `query-persist-restored-query-state` | not run |  | | | |
| `quill-shared-toolbar-focus` | not run |  | | | |
| `returns-validated-error-accumulation` | not run |  | | | |
| `scc-bounded-memory-spilling` | not run |  | | | |
| `scriggo-method-declarations` | not run |  | | | |
| `skrub-duration-encoding` | not run |  | | | |
| `sql-formatter-bigquery-pipe-formatting` | not run |  | | | |
| `sqlfmt-create-table-ddl-formatting` | not run |  | | | |
| `sqlite-utils-safe-import-checkpoints` | not run |  | | | |
| `superjson-error-stack-serialization` | not run |  | | | |
| `task-task-graph-export` | not run |  | | | |
| `tengo-callable-instance-isolation` | not run |  | | | |
| `tengo-destructuring-bindings` | not run |  | | | |
| `termenv-preserve-ansi-resets` | not run |  | | | |
| `testem-bail-on-test-failure` | not run |  | | | |
| `testem-per-launcher-reports` | not run |  | | | |
| `textual-kitty-key-phases` | not run |  | | | |
| `textual-richlog-follow-state` | not run |  | | | |
| `tomlkit-toml-table-converters` | not run |  | | | |
| `true-myth-iterable-collection-combinators` | not run |  | | | |
| `ts-pattern-match-each` | not run |  | | | |
| `updo-policy-alerting` | not run |  | | | |
| `valibot-recursive-schema-composition` | not run |  | | | |
| `vitest-duration-sharding` | not run |  | | | |
| `vulture-persistent-analysis-cache` | not run |  | | | |
| `wasmi-trap-coredumps` | not run |  | | | |
| `wazero-multi-module-snapshots` | not run |  | | | |
| `yaegi-go-embed-directives` | not run |  | | | |
| `yjs-map-conflict-detection` | not run |  | | | |
| `ytt-jsonpath-query-api` | not run |  | | | |

## LoLBench

1 run, 19 not run, 20 harbor tasks.

| question | status | builds | peak_gb | slots | memory_mb |
|----------|--------|--------|---------|-------|-----------|
| `cpython_1` | not run |  | | | |
| `cpython_10` | not run |  | | | |
| `cpython_11` | not run |  | | | |
| `cpython_12` | not run |  | | | |
| `cpython_2` | not run |  | | | |
| `cpython_3` | not run |  | | | |
| `cpython_4` | not run |  | | | |
| `cpython_5` | not run |  | | | |
| `cpython_6` | not run |  | | | |
| `cpython_8` | not run |  | | | |
| `cpython_9` | not run |  | | | |
| `fastapi_1` | not run |  | | | |
| `flink_1` | not run |  | | | |
| `flink_10` | not run |  | | | |
| `flink_11` | not run |  | | | |
| `flink_7` | not run |  | | | |
| `flink_9` | not run |  | | | |
| `kafka_1` | not run |  | | | |
| `kafka_2` | not run |  | | | |
| `ruff_1` | run | 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 | | | |

## SWE-bench Pro

Only the instance P2 has materialized on this worker is listed. The rest of the dataset is omitted until a later P2 writes it under `eval-runs/swebenchpro/tasks`.

| question | status | builds | peak_gb | slots | memory_mb |
|----------|--------|--------|---------|-------|-----------|
| `instance_NodeBB__NodeBB-04998908ba6721d64eba79ae3b65a351dcfbc5b5-vnan` | run | 13, 14, 15, 16 | | | |

