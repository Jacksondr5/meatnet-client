# Operational Readiness Requirements

## Purpose

Define technology-agnostic operational requirements for running the SBC service reliably in a home environment with intermittent power and network conditions.

This document is requirements-only. It does not prescribe specific tools, init systems, or deployment platforms.

## Scope

Applies to runtime operation of the SBC service, including:

- startup and restart behavior
- health visibility and reporting
- local fallback behavior during outages
- recoverability after failures

## Goals

- Service becomes available automatically when power returns.
- Service remains resilient to transient process, network, and backend failures.
- Device health is visible in the web UI whenever network is available.
- Loss of observability is minimized even during outages.

## Non-Goals

- Selecting a specific supervisor/runtime stack.
- Defining full deployment automation.
- Defining full security architecture (covered in security/access docs).

## Functional Requirements

## 1. Startup and Lifecycle

- The service MUST start automatically on device boot.
- The service MUST restart automatically after unexpected termination.
- The runtime MUST avoid unbounded rapid restart loops (backoff or equivalent).
- The system MUST expose current lifecycle state (`starting`, `live`, `degraded`, `syncing`, `error`).

## 2. Connectivity and Check-In

- On startup, the service MUST attempt to establish backend connectivity and identify itself.
- The service MUST periodically publish health check-ins while online.
- The service MUST include freshness metadata (last successful check-in timestamp).

## 3. Failure Reporting

- Runtime failures MUST be captured as structured operational events.
- When backend connectivity exists, operational events MUST be published for UI visibility.
- If backend connectivity is unavailable, operational events MUST be queued locally and replayed later.

## 4. Local Fallback and Durability

- The service MUST maintain a bounded local durable queue for operational events needed for later replay.
- The queue MUST survive process restarts and power loss.
- The system MUST define behavior at local storage limits (including explicit `at_risk` signaling).

## 5. Recovery Behavior

- After network restoration, queued operational events MUST replay in order.
- After process restart, the service MUST recover local state needed for replay and resume normal operation.
- Recovery actions MUST be observable via state transitions and events.

## 6. Health Signals

The service MUST report at least:

- liveness (process running)
- readiness (can perform core duties)
- backend connectivity status
- data replay backlog indicator
- recent error summary

## 7. Configuration and Identity

- The device MUST have a stable runtime identity.
- Critical configuration MUST be validated at startup.
- Invalid critical configuration MUST produce clear error states/events.

## 8. Operator Visibility

- The UI MUST show last seen time, current runtime state, and recent operational errors.
- Health states MUST be explicit and non-ambiguous (no generic single "offline" state).

## Optional Requirement: External Companion Watchdog

A separate lightweight companion process MAY be used to improve resilience and visibility.

If used, it MUST:

- monitor liveness/readiness of the main service,
- trigger recovery actions when the main service is unresponsive,
- persist minimal local breadcrumbs about recovery actions,
- avoid creating conflicting control loops with the primary service lifecycle manager.

If not used, equivalent guarantees MUST still be met by the primary lifecycle mechanism.

## Data Handling During and After Recovery

This document does not redefine data-correctness rules for replay/backfill/recovery writes.

Authoritative behavior is defined in:

- [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md)
- [2026-02-21-error-handling-design.md](./2026-02-21-error-handling-design.md)

Operational designs produced from these requirements MUST preserve the contract-defined guarantees, including:

- durability boundary definitions,
- idempotent replay/materialization behavior,
- ordering and timestamp provenance rules,
- explicit failure/recovery state transitions.

## Reliability Constraints

- The design MUST tolerate routine home-network interruptions.
- The design MUST tolerate abrupt power loss and cold boots.
- The design SHOULD degrade gracefully under backend outages.
- The design SHOULD minimize manual intervention for common failures.

## Acceptance Criteria

A candidate operational design is acceptable when it demonstrates:

1. automatic boot-start and crash-restart behavior,
2. periodic online health check-ins visible in UI,
3. durable offline queuing and replay of operational events,
4. explicit health state transitions during failure and recovery,
5. bounded behavior under repeated failures and storage pressure.

## Next Step

Use this requirements document as input to an implementation-agnostic operational design proposal that evaluates at least two runtime management approaches.

Current project direction: prioritize a containerized SBC runtime for deployment consistency across development Linux hosts and Raspberry Pi devices.

Container-specific runtime constraints and Phase 6 acceptance criteria are defined in:

- [2026-03-06-container-runtime-constraints.md](./2026-03-06-container-runtime-constraints.md)
