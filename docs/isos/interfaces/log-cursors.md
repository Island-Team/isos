# Current-run log cursors — P07

[中文](../../../docs_cn/isos/interfaces/log-cursors_cn.md) · [P06 events](events.md) · [Local API](local-api.md) · [Design](../design/log-cursors.md) · [First use](../usage/get-started.md) · [P07 task](../../../../Island-eos/docs/island/implementation-todo.md)

**Target design — not implemented.** IsOS owns the per-deployment current log scope, retained first event, cursor, rotation and gap diagnostics. IsosUI presents this owner state. The P06 sanitized event envelope and IsOS process status remain authoritative for event content and live health respectively. This P07 extension does not change legacy `GET /api/isos/v1/logs` calls without `view=current`, the schema-1 HTTP envelope, or `isos logs` tail output without `--current`.

## Endpoint and query

`GET /api/isos/v1/logs?view=current&source=all&level=info&limit=100` returns HTTP 200 with the existing `{schema_version:1,ok:true,data:{...}}` envelope. The owner-only loopback Host/Origin checks remain. `view=current` is the only new view value. The optional query fields are:

| Field | Type / unit | Purpose, default, validation and compatibility |
| --- | --- | --- |
| `view` | literal `current` / — | Opts into P07 scope and cursor rules. Omission keeps the existing file cursor and response. Other values reject with HTTP 400 `log_view_invalid`. |
| `source` | source ID or `all` / — | Default `all`; same validated source set as the existing route. Applied after anchor selection. |
| `level` | `trace`, `debug`, `info`, `warn`, `error` / minimum severity | Default `info`; applied after anchor selection, never to the pinned anchor. |
| `node_path` | canonical saved-graph node JSON Pointer / — | Optional exact node filter, at most 512 UTF-8 bytes, ending at `/nodes/<id>`; no member pointer or local path. Null-path events do not match. |
| `correlation_id` | ASCII ID / — | Optional exact correlation filter, 1–64 letters, digits, `_` or `-`; null correlations do not match. |
| `limit` | integer / records | Default 100, range 1–200. The response also scans at most 64 KiB of retained log bytes per call; an empty filtered page may still advance its cursor. |
| `cursor` | opaque string / position | Optional, at most 512 ASCII bytes. P07 tokens begin `lc1.` but clients must not parse or persist their contents. The token authenticates deployment, scope kind/ID, normalized filter set, segment generation and position. Tampering, a legacy inode cursor, wrong deployment or changed filters reject with HTTP 400 `log_cursor_invalid` or `log_cursor_filter_mismatch` and no raw token echo. |

`GET /api/isos/v1/process-status` adds nullable `run_id`, the **last admitted deployment run**. It stays set through stopped, failed, PID loss and `active_revision:null`, and changes only on a new admitted run. `state`, `processes`, `last_error` and active revision remain the live status authority; a log page never makes them ready. The manager-session ID is returned as `scope.run_id` only while no run has ever been admitted for this deployment. IsOS persists the last-run identity with the owner log cursor state so a manager restart cannot silently switch a stopped deployment back to manager scope.

## Response

`data` has exactly these P07 fields in addition to the unchanged P06 event objects inside `anchor.event` and `lines`:

| Field | Type / unit | Meaning |
| --- | --- | --- |
| `scope` | object `{kind,run_id,active_revision}` / — | `kind` is `manager_session` or `deployment_run`; `run_id` is the selected manager `m...` or last admitted deployment `r...` ID, 1–64 ASCII bytes; `active_revision` is the active graph digest string or null and is display metadata only. No caller-selected run ID is accepted. |
| `anchor` | object `{kind,state,event}` / — | `kind` is `first_prelaunch_failure` for manager scope or `first_ready_or_failure` for run scope. `state` is `pending`, `found` or `unavailable`. `event` is a complete P06 stored event only when found, otherwise null. It is returned outside `lines` and ignores all display filters. |
| `lines` | array of P06 stored events / records | At most `limit` events from the same scope **strictly after** the anchor; all display filters are conjunctive. No duplicate anchor in `lines`. Empty while the first anchor is pending. |
| `next_cursor` | opaque string / position | Always present, including empty/pending pages. Poll with it and the same normalized filters. A new run returns a new token. |
| `rotated` | Boolean / — | True when this page crosses a retained segment boundary; retained rotation alone does not imply loss. Kept for existing UI expectations. |
| `diagnostics` | array of `{code,count}` / gaps | Zero or more typed, bounded diagnostics. `count` is a nonnegative integer when known and null when unknown. Never contains raw lines, paths or tokens. |

Diagnostic codes are `log_run_changed` (old run-bound cursor rebased to the current scope), `log_cursor_expired` (segment fell beyond retained backups), `log_collector_gap` (P06 ingest/write drop counter advanced), and `log_anchor_unavailable` (pinned first event could not be persisted or recovered). They are returned in that order if several apply. Run change and expired cursor return HTTP 200 with a fresh scope/anchor/cursor, not a silent empty page; malformed and filter-mismatched tokens return HTTP 400. `rotated:true` can accompany a gap but is not itself a gap. No `diagnostics` entry claims an unknown lost-event count is zero.

A P07 query error preserves the schema-1 envelope: `{"schema_version":1,"ok":false,"data":{"diagnostics":[{"path":"/query/cursor","code":"log_cursor_filter_mismatch","message":"Log cursor filters changed"}]},"error":"log cursor rejected"}` with HTTP 400. `log_cursor_invalid` uses the same cursor path and a fixed safe message; `log_view_invalid` uses `/query/view`. These diagnostics never echo the supplied token or filter values.

An initial manager-session failure page, even with `source=eos&level=error`, is:

```json
{"schema_version":1,"ok":true,"data":{"scope":{"kind":"manager_session","run_id":"m42-abc","active_revision":null},"anchor":{"kind":"first_prelaunch_failure","state":"found","event":{"timestamp":"2026-11-01T10:00:00.000Z","level":"error","source":"packages","core":"isos","node_path":"/cores/eos/layers/process/groups/mujoco/nodes/sim","event_code":"package_load_failed","run_id":"m42-abc","correlation_id":"p3","message":"Verified package input changed; relink the exact release","fields":{"package_id":"island/mujoco-sim","package_version":"0.1.1"},"pid":42}},"lines":[],"next_cursor":"lc1.OPAQUE","rotated":false,"diagnostics":[]}}
```

The anchor is selected **before** source, level, node and correlation filters. Before any eligible event exists, `anchor.state:"pending"`, `anchor.event:null` and `lines:[]`; the returned cursor can be polled until a ready/failure event appears. An anchor storage failure uses `unavailable` plus `log_anchor_unavailable`, never a misleading permanent pending state. After an anchor is found, the first filtered page starts after that record. A UI may render the pinned anchor separately and then append `lines` without deduplicating by message text.

## Scope, retention and recovery

Before the deployment's first admitted run, IsOS selects its current manager session and pins the **first** typed package or Apply failure; a package failure during Run/Restart preflight is eligible; it does not identify this by message text or arbitrary child code. A prelaunch failure is written with the manager-session run ID and optional request correlation before the API/CLI error returns. Once an admitted run exists, IsOS selects that run immediately and retains it after Stop, failed state, process exit, active revision clearing and manager restart. A later admitted run replaces it atomically; a stale cursor then reports `log_run_changed`. There is no P07 clear endpoint. Manager-session errors during an older active/stopped run remain available through the legacy `source=packages|isos` view and status, but do not reassign the current-run scope.

For a deployment run, IsOS pins the first accepted ready code (`deployment_ready`, `eos_ready`, `bridge_ready`, `config_ready`, or reviewed `egia_ready`) **or** the first accepted `error` event from an owner role or IsOS deployment, whichever occurs first. A legacy socket append and raw child line cannot anchor. IsOS copies the sanitized first event into an owner-only `0600` anchor record before acknowledging its collection; this copy survives ordinary `events.log` rotation. Until an eligible record arrives, the anchor is pending and `lines` stays empty. Source, level, node and correlation filters apply only after this scope/anchor choice. A restart/recovery attempt gets its own run and anchor; IsOS never splices old-run lines into the new page.

IsOS reads the current `events.log` plus its configured `1..10` retained backups (`65,536..100,000,000` bytes per file; default 256 KiB and two backups) in chronological order. A cursor bound to a retained segment continues across rotation without loss. An expired segment rebases to the earliest retained record **after** the pinned anchor and reports `log_cursor_expired`; the pinned anchor remains visible. The current manager-session anchor and last-run anchor, run ID, segment identities and bounded loss counters live in private `data/logs/<deployment>/cursor-state.json`, atomically replaced and parent-fsynced. It retains at most those two anchor copies and one current scope per deployment; it is not a second event stream. On manager restart, reconcile it with retained logs and P06 status without inventing missing events. If the collector was unavailable, preserve a known drop count or report null and emit `log_collector_gap` on recovery. A log write or cursor-state failure cannot delay actuator stop/reap; status reports the failure and the page reports the gap.

The CLI adds `isos logs [source] --current --level info --lines 100` for a one-shot current-scope read: print the unfiltered anchor first, then filtered lines and any gap diagnostic to stderr. Without `--current`, the existing tail behavior is unchanged. P07 does not add a graph-authored log route or derive health from event order.
