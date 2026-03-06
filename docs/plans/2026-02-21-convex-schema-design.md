# Convex Schema Design

## Overview

The Convex schema serves as the real-time data store and bidirectional communication layer between the SBC service and the Next.js web app. It must support both high-frequency real-time streaming during cooks and efficient historical queries for cook analysis.

Reliability semantics (durability, idempotency, lease-based command execution) are defined in [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md).

Access model for MVP is single-user household with explicit data ownership boundaries.

## Tables

### `devices`

Registry of known Combustion devices used by MVP.

| Field | Type | Description |
|-------|------|-------------|
| `serialNumber` | string | Unique device identifier (4-byte hex for probes, 10-byte for nodes) |
| `productType` | enum | `probe`, `node` |
| `nodeType` | enum? | `repeater`, `display`, `booster` (for node devices) |
| `ownerId` | string | Household/user owner boundary for all device-scoped queries/mutations |
| `name` | string? | User-assigned friendly name (e.g., "Brisket probe") |
| `firmwareRevision` | string? | Last known firmware version |
| `hardwareRevision` | string? | Last known hardware version |
| `lastSeen` | number | Timestamp of last data received |

### `cookSessions`

One record per cook session per probe. A new session is created when the probe generates a new session ID (happens when removed from charger).

| Field | Type | Description |
|-------|------|-------------|
| `deviceId` | Id\<devices\> | Reference to the device |
| `ownerId` | string | Household/user owner boundary (must match referenced device owner) |
| `sessionId` | number | The probe session ID (random uint32) |
| `startTime` | number | When the session was first detected |
| `endTime` | number? | When the session ended (null if active) |
| `samplePeriodMs` | number | Milliseconds between log entries |
| `metadata` | object? | User-added notes (protein type, weight, cook method, etc.) |

### `temperatureReadings`

Time-series temperature data. This is the highest-volume table.

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | Id\<cookSessions\> | Reference to the cook session |
| `sequenceNumber` | number | Probe log sequence number |
| `timestamp` | number | Estimated sample time (epoch ms, derived from sequence and sample period) |
| `capturedAt` | number | When SBC received or backfilled this reading (epoch ms) |
| `timestampSource` | enum | `liveObserved`, `backfillEstimated`, `repairedEstimated` |
| `timestampConfidence` | enum | `high`, `medium`, `low` |
| `temperatures` | number[] | All 8 sensor temps in C (probe) |
| `virtualCore` | number? | Index of sensor the probe considers "core" |
| `virtualSurface` | number? | Index of sensor the probe considers "surface" |
| `virtualAmbient` | number? | Index of sensor the probe considers "ambient" |

### `predictionSnapshots`

Prediction state changes. Recorded when prediction state changes, not on every temperature reading.

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | Id\<cookSessions\> | Reference to the cook session |
| `timestamp` | number | When this snapshot was captured |
| `state` | enum | `probeNotInserted`, `probeInserted`, `warming`, `predicting`, `removalPredictionDone` |
| `mode` | enum | `none`, `timeToRemoval`, `removalAndResting` |
| `type` | enum | `none`, `removal`, `resting` |
| `setPointCelsius` | number | Target temperature |
| `heatStartCelsius` | number | Core temp when heating began |
| `secondsRemaining` | number | Predicted seconds to target |
| `estimatedCoreCelsius` | number | Estimated current core temp |
| `transitionOrdinal` | number | Monotonic transition number within the session |

### `foodSafetySnapshots`

Food safety state changes. Recorded when food safety state changes.

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | Id\<cookSessions\> | Reference to the cook session |
| `timestamp` | number | When this snapshot was captured |
| `state` | enum | `notSafe`, `safe`, `safetyImpossible` |
| `mode` | enum | `simplified`, `integrated` |
| `product` | string | Food category |
| `logReduction` | number | Achieved log reduction |
| `secondsAboveThreshold` | number | Time spent above safe temp |
| `transitionOrdinal` | number | Monotonic transition number within the session |

### `deviceCommands`

Command queue with full acknowledgement tracking. Commands flow from the web UI through Convex to the SBC and then to BLE devices.

| Field | Type | Description |
|-------|------|-------------|
| `deviceSerialNumber` | string | Target device serial number |
| `ownerId` | string | Household/user owner boundary used for command authorization |
| `commandType` | enum | `setPrediction`, `configFoodSafe`, `resetFoodSafe`, `setAlarms`, `silenceAlarms` |
| `payload` | object | Command-specific parameters |
| `status` | enum | `pending`, `leased`, `sent`, `succeeded`, `failed`, `expired`, `cancelled` |
| `attemptCount` | number | Number of execution attempts so far |
| `maxAttempts` | number | Maximum allowed attempts for this command |
| `leasedBy` | string? | SBC instance identifier holding the current lease |
| `leaseVersion` | number? | Monotonic lease version for compare-and-set transitions |
| `leaseExpiresAt` | number? | When the current lease expires |
| `createdAt` | number | When the web UI created the command |
| `sentAt` | number? | When the SBC wrote it to BLE |
| `completedAt` | number? | When the device response arrived |
| `requestId` | number? | The UART request ID (uint32) used for response matching |
| `responseId` | number? | The UART response ID from the device |
| `ttlSeconds` | number | How long the command is valid (default: 30) |
| `expiresAt` | number | `createdAt + ttlSeconds * 1000` |
| `reasonCode` | enum? | `expired`, `ble_unavailable`, `device_timeout`, `protocol_error`, `validation_error`, `cancelled_by_user` |
| `error` | string? | Additional details if failed |

### `networkTopology`

Mesh snapshots from node list + topology responses. Used for network diagnostics, not cook-session analytics.

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | number | When this topology snapshot was captured |
| `gatewaySerialNumber` | string | Node serial number of the connected gateway |
| `nodeSerialNumber` | string | Node being described in this snapshot |
| `nodeType` | enum | `repeater`, `display`, `booster`, `unknown` |
| `inboundConnections` | object[] | Array of `{ serialNumber, productType, rssi }` |
| `outboundConnections` | object[] | Array of `{ serialNumber, productType, rssi }` |
| `health` | enum | `healthy`, `degraded`, `offline` |

Default health thresholds used to compute `health`:

- `healthy`: heartbeat age `<= 15s`, median RSSI `>= -75 dBm`, hop count `<= 2`
- `degraded`: heartbeat age `> 15s` and `<= 45s`, or median RSSI in `[-90, -75) dBm`, or hop count `>= 3`
- `offline`: heartbeat age `> 45s`
- State changes require `2` consecutive evaluations before persisting

### `heartbeats`

Raw heartbeat stream from node heartbeat messages for freshness and link quality monitoring.

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | number | When heartbeat was received by SBC |
| `nodeSerialNumber` | string | Reporting node |
| `macAddress` | string | Node MAC address |
| `hopCount` | number | Hops this message has traveled |
| `direction` | enum | `inbound`, `outbound` |
| `connections` | object[] | Array of `{ serialNumber, productType, rssi, attributes }` |

#### Command Status Progression

```
pending → leased → sent → succeeded
                     ↘
                      failed / expired / cancelled

At any step, terminal transitions may occur when applicable:
- pending/leased → expired (TTL exceeded)
- leased → failed (BLE connection unavailable, protocol error, validation error)
- sent → failed (device timeout or explicit device error)
- pending/leased → cancelled (user initiated)
```

#### Command Acknowledgement Flow

The protocol has built-in acknowledgements at every layer. In MVP we only send probe-targeted commands (never global alarm silence), so each command expects a single response and maps cleanly to one `requestId` + one `responseId`.

The web UI can display this as a progress indicator:

```
Command: Set prediction to 95C on Probe #1
[x] Queued          (14:32:01)
[x] SBC leased      (14:32:01)  <- command owned for execution
[x] Sent to device  (14:32:02)  <- BLE write confirmed
[x] Device confirmed (14:32:02) <- UART response success (`succeeded`)
```

#### Timeout Handling

| Transition | Timeout | Behavior |
|-----------|---------|----------|
| `pending` → `leased` | 30s (client-side expectation) | Web UI may show command queue delay; command remains eligible for lease until expiry. |
| `leased` → `sent` | 10s | SBC sets terminal failure with `reasonCode=\"ble_unavailable\"` when BLE path cannot be established. |
| `sent` → `succeeded`/`failed` | 5s | SBC sets terminal failure with `reasonCode=\"device_timeout\"` when response does not arrive in window. |
| Any → expired | `ttlSeconds` | SBC checks `expiresAt` before executing. Skips expired commands with error: `"expired"` |

## Key Design Decisions

- **Temperature readings are separate from predictions/food safety** — Temps arrive every few seconds and are high volume. Predictions and food safety only need to be recorded when their state changes, keeping those tables smaller and queries faster.

- **`sequenceNumber` on temperature readings** — Maps directly to the probe's internal log sequence. Makes backfill straightforward: query "what's my max sequence number for this session?" and request logs starting from there.

- **Dual timestamps for backfill correctness** — Log backfill responses do not include wall-clock time. Store both `timestamp` (estimated sample time) and `capturedAt` (ingestion time) plus `timestampSource` so charts stay continuous while preserving provenance.
- **Deterministic timestamp anchoring** — For backfill, anchor reconstruction to the first post-reconnect live Probe Status sequence/timestamp pair and derive historical sample times from `samplePeriodMs`.
- **Timestamp confidence provenance** — Persist `timestampConfidence` to distinguish high-confidence live timestamps from temporary-anchor estimates.

- **Mesh telemetry stored separately** — Heartbeats/topology are operational diagnostics and should not be coupled to cook sessions. Separate tables keep cook analytics clean while enabling a network health UI.

- **Commands table as a lease-backed queue** — Lease-based state machine (`pending` → `leased` → `sent` → terminal) with compare-and-set transitions, timestamps, retries, and typed terminal reasons.
- **Single-user ownership boundaries** — MVP is single-user household, but all device/session/command writes include `ownerId` and all reads are owner-scoped to prevent cross-data mixing.

- **Snapshot tables for state changes** — Prediction and food safety data is captured only when state changes, not on every temperature reading. This keeps the data manageable while still providing a complete timeline of the cook.

## Required Indexes and Uniqueness Constraints

- `temperatureReadings`: unique on `(sessionId, sequenceNumber)` for idempotent replay.
- `predictionSnapshots`: unique on `(sessionId, transitionOrdinal)`.
- `foodSafetySnapshots`: unique on `(sessionId, transitionOrdinal)`.
- `deviceCommands`: command id is unique identity; state transitions must enforce compare-and-set on `(id, leaseVersion, status)`.
