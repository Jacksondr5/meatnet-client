# Error Handling and Resilience Design

## Overview

This document describes failure handling for the MeatNet companion runtime.

Authoritative reliability semantics (durability, idempotency, command leasing, and UI states) are defined in [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md).

## Reliability Guarantees (MVP)

- SBC -> Convex delivery is at-least-once with idempotent materialization by key.
- A record is durable when persisted in Convex or appended to the SBC durable local spool.
- In-memory buffering alone is not durable.
- Historical recovery is strong but finite; probe log retention is not infinite.

## Failure 1: BLE Node Connection Drops

**When it happens:** Node goes out of range, loses power, firmware crash, or BLE interference.

**Detection:** GATT disconnect event and sustained missing heartbeat/probe-status traffic.

**Recovery strategy:**

1. **Enter degraded mode:** Keep ingesting advertising data and mark reduced-fidelity records.
2. **Reconnect to same node:** Exponential backoff (1s, 2s, 4s, 8s, capped at 30s).
3. **Fail over to different node:** If original node is unavailable and another node is visible.
4. **Backfill on reconnect:** Request missing ranges by sequence and reconcile idempotently.
5. **Exit degraded mode:** Only after backfill catch-up window succeeds.

**What the web app sees:** `Degraded` during advertising-only period, then `Syncing` during catch-up, then `Live`.

## Failure 2: Internet Connectivity Loss

**When it happens:** SBC loses WiFi, router reboots, or ISP outage.

**Detection:** Convex writes fail/time out.

**Recovery strategy:**

1. **Durable spooling:** Continue ingest and append outbound events to local durable spool.
2. **Controlled command handling:** Do not lease new commands unless status transitions can be durably recorded.
3. **Replay on reconnect:** Flush spool in chronological order with idempotent upserts.
4. **Backpressure policy:** Prioritize cook-critical streams over diagnostics when storage pressure rises.

**What the web app sees:** `Syncing` when backlog replay starts; optional `At Risk` if local durability pressure rises.

## Failure 3: SBC Process Crash or Reboot

**When it happens:** Panic, OS restart, power loss, manual restart.

**Recovery strategy:**

1. Recover and replay durable local spool.
2. Re-establish BLE/node connection.
3. Compute missing sequence ranges from Convex materialized maxima.
4. Backfill missing probe logs.
5. Resume steady-state ingest and command polling.

**What the web app sees:** temporary gap followed by `Syncing` while replay/backfill completes.

## Failure 4: Convex Service Degradation

**When it happens:** Timeouts, rate limiting, auth failures, or validation failures.

**Detection:** Error-classified Convex mutation/query failures.

**Recovery strategy:**

1. **Timeout/rate-limit:** retry with capped exponential backoff and spool growth monitoring.
2. **Auth/validation:** stop affected pipeline, emit hard error, and avoid infinite retries.
3. **Recovery:** replay spooled events once service health returns.

## Summary

| Failure | System Behavior | Recovery | Residual Risk |
|---------|-----------------|----------|---------------|
| BLE disconnect | Degraded fidelity via advertising fallback | Reconnect + backfill | Extended disconnect may exceed probe log retention |
| Internet loss | Durable local spooling | Replay on reconnect | Storage pressure if outage is prolonged |
| SBC crash | Spool replay + backfill | Automatic on restart | Minimal if spool is healthy |
| Convex degradation | Retry/spool or fail-fast by error class | Automatic/manual depending on class | Prolonged outage can trigger `At Risk` |
