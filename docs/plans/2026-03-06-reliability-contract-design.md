# Reliability Contract Design

## Overview

This document defines the authoritative correctness contract for data capture, command execution, and recovery behavior across the MeatNet companion system.

It is the source of truth for:

- SBC runtime behavior
- Convex schema/mutation constraints
- Web app status semantics
- Reliability-focused testing

This document complements and constrains:

- [2026-02-21-error-handling-design.md](./2026-02-21-error-handling-design.md)
- [2026-02-21-sbc-service-design.md](./2026-02-21-sbc-service-design.md)
- [2026-02-21-convex-schema-design.md](./2026-02-21-convex-schema-design.md)

## Goals

- Prevent silent data corruption or duplicate writes during retries/recovery.
- Ensure deterministic behavior under disconnects, crashes, and replay.
- Make user-visible status match backend truth.
- Make guarantees measurable via tests and metrics.

## Non-Goals

- Exactly-once delivery over the network.
- Guaranteeing infinite historical recovery from probe memory.
- Replacing existing architecture components.

## Reliability Model

The system uses:

- At-least-once delivery for SBC -> Convex writes.
- Exactly-once materialization by deterministic idempotency keys.
- Explicit degraded-mode states instead of implicit assumptions.

## Data Classes and Guarantees


| Data Class               | Source                           | Delivery                     | Idempotency Basis                                                               | Loss Boundary                                                                                   | Notes                        |
| ------------------------ | -------------------------------- | ---------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------- |
| Temperature readings     | Probe status + probe logs        | At least once                | `(deviceSerial, probeSessionId, sequenceNumber)`                                | Loss possible only if probe log overwritten before recovery and data not in local durable spool | Primary cook timeline        |
| Prediction snapshots     | Probe status-derived transitions | At least once                | `(deviceSerial, probeSessionId, predictionStateFingerprint, transitionOrdinal)` | Same as above                                                                                   | State-change snapshots only  |
| Food safety snapshots    | Probe status-derived transitions | At least once                | `(deviceSerial, probeSessionId, foodSafeStateFingerprint, transitionOrdinal)`   | Same as above                                                                                   | State-change snapshots only  |
| Command records          | Web app mutations + SBC updates  | At least once status updates | Command `id` + monotonic state transition constraints                           | Command may expire before execution                                                             | See command contract         |
| Mesh heartbeats/topology | Node stream                      | Best effort                  | `(nodeSerial, timestampBucket, direction)` as dedup heuristic                   | May be dropped under sustained outage/backpressure                                              | Operational diagnostics only |


## Durability Boundaries

A record is considered durable when one of the following is true:

1. It is persisted in Convex and acknowledged by mutation success.
2. It is appended to the local durable spool on SBC.

In-memory buffering alone is not durable.

### Local Durable Spool Contract

- SBC maintains an append-only local spool for outbound Convex events.
- Spool entries are fsync-backed before acknowledged as accepted by ingest pipeline.
- Spool is replayed in order on startup and after connectivity recovery.
- Entries are tombstoned/deleted only after confirmed Convex materialization.

### Spool Capacity and Overflow

- Configurable max spool size (default target: 512MB).
- On overflow, eviction priority is diagnostics-first:
  1. drop `heartbeats`
  2. drop `networkTopology`
  3. retain cook-critical streams as long as possible
- If cook-critical data cannot be spooled, emit critical health event and surface explicit "data loss risk" state.

## Idempotency and Uniqueness Contract

## Canonical Keys

- `temperatureReadings`: unique on `(deviceSerialNumber, probeSessionId, sequenceNumber)`
- `predictionSnapshots`: unique on `(deviceSerialNumber, probeSessionId, transitionOrdinal)`
- `foodSafetySnapshots`: unique on `(deviceSerialNumber, probeSessionId, transitionOrdinal)`
- `deviceCommands`: unique command identity is existing command document id; state transitions are guarded by compare-and-set checks.

`sequenceNumber` alone is never treated as globally unique.

## Mutation Behavior

All ingest mutations must be idempotent upserts keyed by canonical keys.

- Duplicate write -> no-op (or safe overwrite of identical payload).
- Payload mismatch on same key -> reject + emit integrity error metric/event.

## Ordering Contract

- Logical order for cook timeline is by `sequenceNumber` within `(deviceSerial, probeSessionId)`.
- `timestamp` is display-oriented sample time; `capturedAt` is ingestion time.
- Late arrivals are accepted if key is new.

## Command Reliability Contract

## Command States

`pending -> leased -> sent -> succeeded | failed | expired | cancelled`

`leased` replaces ambiguous "received" and means a specific SBC instance owns execution lease.

## Lease Rules

- Lease uses `(leasedBy, leaseVersion, leaseExpiresAt)`.
- Product invariant: exactly one provisioned SBC per deployment; multi-SBC active coordination is not a supported mode.
- SBC acquires lease with compare-and-set from `pending` to `leased`.
- Only lease owner can move command forward.
- If lease expires before terminal state, command may be re-leased (subject to retry policy).

## Retry and Expiry Rules

- Each command type declares `maxAttempts` and `retryableFailures`.
- Expired commands are terminal (`expired`) and never auto-retried.
- Non-expired retryable failures transition back to `pending` with incremented `attemptCount` until budget exhausted.
- Terminal failures are explicit `failed` with typed reason code.

## Terminal Reason Codes

Minimum set:

- `expired`
- `ble_unavailable`
- `device_timeout`
- `protocol_error`
- `validation_error`
- `cancelled_by_user`

UI semantics must map directly to these typed reasons, not free-form strings.

## Time and Timestamp Contract

## Timestamp Fields

- `timestamp`: estimated sample time used for charts.
- `capturedAt`: SBC observation/ingest time.
- `timestampSource`: `liveObserved | backfillEstimated | repairedEstimated`.
- `timestampConfidence`: `high | medium | low`.

## Reconstruction Rules

- Preferred anchor: first post-reconnect live probe status with known sequence.
- Temporary-anchor writes are allowed with `timestampConfidence=low`.
- MVP policy: no automatic timestamp rewrites after persistence.
- Future option (non-MVP): manual/admin repair tooling may upgrade low-confidence points to `repairedEstimated` when justified.

## Repair Policy (MVP)

- Persist provenance on first write (`timestampSource`, `timestampConfidence`).
- Do not run background repair jobs.
- If manual repair tooling is added later, it must be explicitly invoked and auditable.

## Recovery Contracts by Failure Mode

## BLE Node Disconnect

- Enter `degraded_data_fidelity` mode.
- Continue ingest from advertising stream.
- Mark records captured during degraded mode with fidelity flags.
- On reconnect, run backfill and clear degraded mode only after successful catch-up window.

## Internet or Convex Outage

- Continue full ingest into durable spool.
- Command execution continues only for already leased commands where safe; new command leases paused if command status cannot be durably written.
- Replay spool chronologically on recovery with idempotent writes.

## SBC Crash/Reboot

Startup order:

1. recover and replay durable spool
2. re-establish BLE/node connection
3. compute missing ranges from Convex materialized max sequence
4. request backfill
5. resume steady-state ingest

## Convex Degradation

- Classify failure type: auth, validation, rate-limit, timeout/unavailable.
- Rate limit/timeouts -> retry with capped exponential backoff.
- Auth/validation -> stop affected pipeline, emit hard error, avoid infinite retry loops.

## User-Visible Guarantees

The UI should communicate explicit states:

- `Live`: full-fidelity live data.
- `Degraded`: partial data (for example advertising-only temps).
- `Syncing`: backlog replay/backfill in progress.
- `At Risk`: local durability pressure threatens retention.

No generic "offline" banner should hide distinct failure semantics.

## Observability Requirements

Minimum metrics:

- spool size bytes and oldest entry age
- replay throughput and replay lag
- dedup hit rate by table
- command lease contention and retry counts
- command terminal reasons (typed)
- timestamp confidence distribution
- degraded mode duration

Minimum logs/events:

- mode transitions (`Live/Degraded/Syncing/At Risk`)
- spool overflow or cook-critical drop risk
- payload mismatch on idempotency key
- manual repair invocations (if enabled) with affected counts

## Test Requirements Tied to Contract

- Idempotent replay test: same batch ingested N times -> stable materialized result.
- Crash window test: crash after spool append but before Convex write -> replay correctness.
- Partial flush test: mid-batch Convex failure -> safe retry with no duplicates.
- Command lease race test: two workers contend -> single owner executes.
- Command expiry boundary test: lease near expiry does not execute stale command.
- Timestamp provenance test: temporary-anchor data is persisted with correct source/confidence metadata.
- Spool overflow policy test: diagnostics dropped before cook-critical streams.

## Required Follow-Up Updates

- Update [2026-02-21-error-handling-design.md](./2026-02-21-error-handling-design.md) to remove absolute "no data loss" wording and reference this contract.
- Update [2026-02-21-convex-schema-design.md](./2026-02-21-convex-schema-design.md) with explicit uniqueness/index constraints and leased command fields.
- Update [2026-02-21-sbc-service-design.md](./2026-02-21-sbc-service-design.md) startup/recovery and command handler sections to match lease + spool model.
- Update [2026-02-21-web-app-design.md](./2026-02-21-web-app-design.md) status/alerts to reflect explicit reliability states and typed command outcomes.

## Bottom Line

The architecture stands. Correctness depends on implementing this contract as written: durable spool boundary, key-based idempotency, lease-based command execution, and explicit time/fidelity provenance.
