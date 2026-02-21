# Error Handling and Resilience Design

## Overview

The MeatNet companion system has four primary failure modes. The fundamental resilience guarantee is that **no cook data is ever lost** — the probe stores all log data on-device, so the SBC can always backfill from probe memory after any failure.

## Failure 1: BLE Node Connection Drops

**When it happens:** Node goes out of range, loses power, firmware crash, or BLE interference.

**Detection:** BLE library reports a disconnect event on the GATT connection. Missing heartbeats and a sustained absence of Probe Status messages from the node are early warnings. Default health thresholds: degraded after 15s without heartbeat, offline after 45s.

**Recovery strategy:**

1. **Immediate fallback:** The passive BLE advertising scanner is already running in parallel. Basic temperature data continues flowing to Convex without interruption. The web app sees temperatures but loses predictions, food safety, alarms, and commands.
2. **Reconnect to same node:** Attempt reconnection with exponential backoff (1s, 2s, 4s, 8s, capped at 30s).
3. **Reconnect to different node:** If the same node doesn't come back within 60 seconds and other nodes are visible via advertising, attempt connecting to a different node. This handles the case where the primary node dies but another node can still relay probe data.
4. **Post-reconnect backfill:** Once reconnected, trigger log backfill for all active sessions. The session manager compares its last known sequence number per probe against the probe's current log range and requests the missing records.

**What the web app sees:** During the disconnected period, temperature readings continue from advertising but with a flag indicating reduced data (no predictions, food safety, or alarms). The network diagnostics page marks gateway status as degraded/offline based on heartbeat freshness. Once reconnected, the gap is backfilled and the UI returns to full fidelity.

## Failure 2: Internet Connectivity Loss

**When it happens:** SBC loses WiFi, router reboots, ISP outage.

**Detection:** Convex client reports connection failure or mutations start timing out.

**Recovery strategy:**

1. **Local buffer:** The SBC continues collecting BLE data normally and writes to an in-memory buffer with disk spillover if the buffer exceeds 50MB. All data that would have gone to Convex is queued.
2. **Command queue frozen:** Pending commands remain in "received" state. New commands from the web app can't reach the SBC.
3. **Automatic reconnect and flush:** The Convex client handles reconnection automatically. Once connectivity returns, the SBC flushes the buffer to Convex in chronological order using batch writes.
4. **Ordering guarantee:** Buffered data includes original timestamps from when the BLE data was received, so historical queries remain accurate after a delayed flush.

**What the web app sees:** Data stops arriving. The dashboard detects this by checking the latest timestamp for each active session. If no new data arrives within the expected sample period, it shows "SBC offline — data will sync when connection resumes." All data appears retroactively once connectivity returns.

## Failure 3: SBC Process Crash or Reboot

**When it happens:** Unhandled exception, OS update, power loss, manual restart.

**Detection:** The process restarts and needs to recover state.

**Recovery strategy:**

1. **No persistent local state required.** On startup, the SBC queries Convex for all active probe sessions (sessions with no `endTime`). For each, it knows the probe serial number and can look for that probe via BLE through the connected node.
2. **Re-establish BLE connections:** Scan for nodes, connect to one, re-subscribe to UART notifications.
3. **Backfill gaps:** For each active session, query the last sequence number in Convex's `temperatureReadings` table. Compare against the probe's current log range. Request any missing logs.
4. **Resume normal operation:** Start pushing data to Convex.

**What the web app sees:** A gap in data that gets backfilled once the SBC is back. SBC startup time should be fast (seconds).

## Failure 4: Convex Service Degradation

**When it happens:** Convex platform issues, rate limiting, mutation failures.

**Detection:** Convex client reports errors on mutations.

**Recovery strategy:**

1. **Buffer locally and retry** — same as internet loss. Convex mutations are idempotent if we use the sequence number as a deduplication key.
2. **Dynamic backoff on rate limiting** — if Convex rate limits are hit (unlikely with proper batching), increase the batch interval dynamically (2s → 5s → 10s).

## Summary

| Failure | Data Impact | Recovery Time | Data Loss |
|---------|------------|---------------|-----------|
| BLE disconnect | Reduced to advertising-only temps | Seconds to reconnect, then backfill | None (backfill covers gap) |
| Internet loss | SBC buffers locally | Automatic on reconnect, flush buffer | None (buffered) |
| SBC crash | No data during downtime | Seconds to restart, then backfill | None (probe stores logs on-device) |
| Convex degradation | SBC buffers locally | Automatic on recovery | None (buffered + idempotent) |
