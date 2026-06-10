# Kafka Message Schemas

## Overview

XZardgz watcher mode uses a CloudEvents-inspired message schema serialized as
JSON. Messages are not strictly CloudEvents-compliant but follow the same
general envelope pattern: a set of provenance fields identifying the event
source and type, plus a payload carrying the analysis request or result.

All messages are UTF-8 JSON objects. No binary encoding is used. Field names use
`snake_case`. Optional fields are omitted when null rather than serialized as
`null` unless otherwise noted.

Two message types exist:

- `WatcherTaskMessage`: produced by external systems and consumed by the watcher
  on the task topic.
- `WatcherResultMessage`: produced by the watcher and published to the result
  topic after each task completes or fails.

## Task Message Schema

`WatcherTaskMessage` is the incoming task envelope. It carries everything the
watcher needs to dispatch a plugin run: provenance, repository target, plugin
selection, provider override, and reply routing.

### Fields

| Field                      | Type             | Required | Description                                                                                                                              |
| -------------------------- | ---------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                       | string           | Yes      | Unique identifier for this task message.                                                                                                 |
| `spec_version`             | string           | Yes      | Schema version string, currently `"1"`.                                                                                                  |
| `event_type`               | string           | Yes      | Event type string. Must be one of the recognized task event types.                                                                       |
| `source`                   | string           | Yes      | URI or identifier of the system that produced this message.                                                                              |
| `repository`               | string           | Yes      | Repository path or clone URL to be analysed.                                                                                             |
| `target_branch`            | string           | No       | Branch to check out. Uses the repository default branch when omitted.                                                                    |
| `plugin`                   | string           | Yes      | Plugin identifier, for example `technical-review` or `security-review`.                                                                  |
| `plugin_config`            | object           | No       | Plugin-specific configuration as a JSON object. Schema is plugin-defined.                                                                |
| `provider`                 | string           | No       | AI provider override, for example `openai` or `anthropic`.                                                                               |
| `model`                    | string           | No       | AI model override. Applied together with `provider` when set.                                                                            |
| `dry_run`                  | bool             | No       | When `true`, the watcher validates the task and returns a result without performing external AI calls. Defaults to `false`.              |
| `workspace_directory`      | string           | No       | Override the workspace directory path for this task.                                                                                     |
| `metadata`                 | object           | No       | Arbitrary string key-value pairs used for routing, tagging, and matcher filtering.                                                       |
| `requested_report_formats` | array of strings | No       | Report formats to produce, for example `["markdown", "json"]`. Plugin defaults apply when omitted.                                       |
| `correlation_id`           | string           | No       | Opaque identifier used to correlate this task with its result and with upstream systems. Passed through unchanged to the result message. |
| `reply_topic_override`     | string           | No       | Override the configured result topic for this specific task's result message.                                                            |

### Task Message Structure

```json
{
  "id": "string",
  "spec_version": "string",
  "event_type": "string",
  "source": "string",
  "repository": "string",
  "target_branch": "string or omitted",
  "plugin": "string",
  "plugin_config": { "...": "..." },
  "provider": "string or omitted",
  "model": "string or omitted",
  "dry_run": false,
  "workspace_directory": "string or omitted",
  "metadata": { "key": "value" },
  "requested_report_formats": ["markdown", "json"],
  "correlation_id": "string or omitted",
  "reply_topic_override": "string or omitted"
}
```

## Result Message Schema

`WatcherResultMessage` is published to the result topic after each task
completes, whether the run succeeded or failed. Consumers should inspect the
`success` field before reading result data fields.

### Result Fields

| Field                | Type              | Always Present | Description                                                                                                               |
| -------------------- | ----------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `correlation_id`     | string            | No             | Correlation identifier from the originating task. Omitted if the task did not include one.                                |
| `original_task_id`   | string            | Yes            | The `id` field from the originating `WatcherTaskMessage`.                                                                 |
| `workspace_id`       | string            | Yes            | Unique identifier for the workspace created for this task.                                                                |
| `started_at`         | string (ISO 8601) | Yes            | Timestamp when the watcher began processing the task.                                                                     |
| `completed_at`       | string (ISO 8601) | Yes            | Timestamp when processing finished, regardless of outcome.                                                                |
| `success`            | bool              | Yes            | `true` if the task completed without errors, `false` otherwise.                                                           |
| `errors`             | array of strings  | Yes            | Error messages. Empty on success.                                                                                         |
| `diagnostics`        | array of objects  | Yes            | Internal diagnostic entries collected during the run. Empty when none.                                                    |
| `findings_summary`   | object            | No             | Summary of findings produced by the plugin. Omitted when no findings were generated.                                      |
| `risk_band`          | string            | No             | Overall risk classification string, for example `low`, `medium`, `high`, or `critical`.                                   |
| `sarif_path`         | string            | No             | Path to the SARIF report file within the workspace. Present only when the security review plugin produced a SARIF report. |
| `workspace_path`     | string            | No             | Absolute path to the workspace directory on disk.                                                                         |
| `scan_artifact_path` | string            | No             | Path to the scan artifact JSON file within the workspace.                                                                 |
| `report_paths`       | object            | No             | Map of report format name to output file path, for example `{"markdown": "/path/to/report.md"}`.                          |
| `provider_metadata`  | object            | No             | Metadata returned by the AI provider, such as token usage. Structure is provider-specific.                                |
| `model_id`           | string            | No             | Identifier of the model used during the run.                                                                              |

### Result Message Structure

```json
{
  "correlation_id": "string or omitted",
  "original_task_id": "string",
  "workspace_id": "string",
  "started_at": "2025-01-15T10:00:00Z",
  "completed_at": "2025-01-15T10:02:30Z",
  "success": true,
  "errors": [],
  "diagnostics": [],
  "findings_summary": {
    "total": 7,
    "by_severity": {
      "critical": 0,
      "high": 2,
      "medium": 3,
      "low": 2
    }
  },
  "risk_band": "medium",
  "sarif_path": "string or omitted",
  "workspace_path": "string or omitted",
  "scan_artifact_path": "string or omitted",
  "report_paths": {
    "markdown": "/path/to/report.md",
    "json": "/path/to/report.json"
  },
  "provider_metadata": {},
  "model_id": "string or omitted"
}
```

## FindingsSummary Schema

`FindingsSummary` is nested inside `WatcherResultMessage.findings_summary`.

| Field         | Type   | Description                                                             |
| ------------- | ------ | ----------------------------------------------------------------------- |
| `total`       | int    | Total number of findings produced by the plugin.                        |
| `by_severity` | object | Map of severity label string to finding count. Keys are plugin-defined. |

Common severity labels are `critical`, `high`, `medium`, `low`, and
`informational`. Not all labels need to be present; a label is omitted when its
count is zero.

```json
{
  "total": 5,
  "by_severity": {
    "high": 2,
    "medium": 2,
    "low": 1
  }
}
```

## Event Type String Constants

| Constant Name                   | String Value                      | Message Direction    |
| ------------------------------- | --------------------------------- | -------------------- |
| `EVENT_TECHNICAL_REVIEW_TASK`   | `xzardgz.technical_review.task`   | Consumed by watcher  |
| `EVENT_TECHNICAL_REVIEW_RESULT` | `xzardgz.technical_review.result` | Published by watcher |
| `EVENT_SECURITY_REVIEW_TASK`    | `xzardgz.security_review.task`    | Consumed by watcher  |
| `EVENT_SECURITY_REVIEW_RESULT`  | `xzardgz.security_review.result`  | Published by watcher |

The watcher uses `WatcherEventType::from_event_str` to parse the `event_type`
field. Any string not listed above returns `None` and the message is rejected.
Event type strings are case-sensitive.

## Example: Task Message for Technical Review

```json
{
  "id": "01HW9K3VZPX4QFMTBRNCD52E7G",
  "spec_version": "1",
  "event_type": "xzardgz.technical_review.task",
  "source": "https://ci.example.com/pipeline/build/4821",
  "repository": "https://github.com/example/myapp.git",
  "target_branch": "main",
  "plugin": "technical-review",
  "plugin_config": {
    "max_findings": 20,
    "severity_threshold": "medium",
    "focus_areas": ["reliability", "maintainability"]
  },
  "provider": "openai",
  "model": "gpt-4.1-mini",
  "dry_run": false,
  "metadata": {
    "triggered_by": "push",
    "pr_number": "412"
  },
  "requested_report_formats": ["markdown", "json"],
  "correlation_id": "ci-build-4821"
}
```

## Example: Result Message for a Successful Technical Review

```json
{
  "correlation_id": "ci-build-4821",
  "original_task_id": "01HW9K3VZPX4QFMTBRNCD52E7G",
  "workspace_id": "01HW9K4ABCDEF12345678",
  "started_at": "2025-01-15T14:22:01Z",
  "completed_at": "2025-01-15T14:24:38Z",
  "success": true,
  "errors": [],
  "diagnostics": [],
  "findings_summary": {
    "total": 4,
    "by_severity": {
      "medium": 3,
      "low": 1
    }
  },
  "risk_band": "medium",
  "workspace_path": "/var/xzardgz/workspaces/01HW9K4ABCDEF12345678",
  "scan_artifact_path": "/var/xzardgz/workspaces/01HW9K4ABCDEF12345678/scan.json",
  "report_paths": {
    "markdown": "/var/xzardgz/workspaces/01HW9K4ABCDEF12345678/reports/report.md",
    "json": "/var/xzardgz/workspaces/01HW9K4ABCDEF12345678/reports/report.json"
  },
  "model_id": "gpt-4.1-mini"
}
```

## Example: Result Message for a Failed Run

A failed result message has `success: false` and one or more entries in
`errors`. Artifact paths and findings data may be absent.

```json
{
  "correlation_id": "ci-build-4822",
  "original_task_id": "01HW9K5MNPQ7RSTUVWXY01ZA",
  "workspace_id": "01HW9K5DEFGH98765432",
  "started_at": "2025-01-15T14:30:00Z",
  "completed_at": "2025-01-15T14:30:04Z",
  "success": false,
  "errors": [
    "Plugin 'technical-review' is not registered in the plugin registry"
  ],
  "diagnostics": [
    {
      "level": "error",
      "message": "Plugin validation failed: unknown plugin identifier",
      "source": "watcher::executor"
    }
  ]
}
```

## Security Notes

### No Secrets in Messages

Task and result messages must not contain API keys, passwords, tokens, or any
other credential material. The `plugin_config` field carries plugin
configuration, not secrets. Provider API keys are supplied through environment
variables on the watcher host and are referenced by name in the configuration
file, not transmitted in messages.

### Correlation ID for Tracing

The `correlation_id` field is designed for distributed tracing and audit
logging. Use it to link a result message back to the upstream system event that
produced the task. The watcher passes the value through unchanged; it does not
validate or generate correlation identifiers.

Keep correlation identifiers opaque and avoid embedding sensitive data such as
user identifiers or internal system paths in them.

### Message Integrity

The Kafka transport does not provide application-level message signing in this
release. If message integrity is required, configure Kafka with TLS
(`security_protocol: SSL` or `SASL_SSL`) to protect messages in transit and
enforce access control at the broker level to restrict who can produce to the
task topic.
