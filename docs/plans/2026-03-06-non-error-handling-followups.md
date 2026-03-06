# Non-Error-Handling Follow-Ups

This note tracks important design items outside of resilience/error handling so they are not lost while we focus on reliability.

## 1. Security and Access Model

- Define user identity model (single-user household vs multi-user shared org).
- Define data ownership boundaries for `devices`, `cookSessions`, and command authorization.
- Define SBC-to-Convex credential lifecycle (provisioning, rotation, revocation).
- Decide debug endpoint policy (`/debug`, `/ws`): local-only binding, auth requirement, and disable-in-production mode.

## 2. Data Model and Query Scalability

- Add explicit Convex indexes and uniqueness constraints for high-volume tables.
- Specify downsampling behavior for long cook charts (server-side policy and thresholds).
- Define retention policy for diagnostics tables (`heartbeats`, `networkTopology`) to control growth.

## 3. Command UX and Product Semantics

- Clarify user-visible behavior when commands expire vs fail vs are retried.
- Define which commands are safe to auto-retry and which require explicit user confirmation.
- Standardize command progress terminology in UI and backend statuses.

## 4. Operational Readiness

- Define deployment/runtime management on SBC (service manager, restart policy, update path).
- Add baseline observability plan (structured logs, metrics, health endpoints, alert thresholds).
- Define incident triage workflow for “data gap,” “command failures,” and “network degraded” reports.

## 5. Testing Coverage Gaps (Non-Error)

- Add one end-to-end smoke path for command roundtrip (UI -> Convex -> SBC -> device ack -> UI).
- Add schema-level tests for indexes/constraints once finalized.
- Add chart/query performance tests for long-session history rendering.
