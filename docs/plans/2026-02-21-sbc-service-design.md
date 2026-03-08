# SBC Service Design

## Overview

The SBC service is a Rust process that runs on a Raspberry Pi near the grill. It bridges the BLE protocol world with the Convex cloud backend using bluer for BLE communication and reqwest for Convex HTTP API calls.

Runtime reliability and recovery semantics are defined in [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md).

Canonical device identity is exact Combustion `productType + serialNumber` from protocol data. The serial number must be normalized by advertisement family or GATT Device Information data. BLE addresses and library-specific peripheral handles are transport-only and must not be used as durable identifiers.

## Internal Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         SBC Service                             │
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │  BLE Scanner  │    │ Node Gateway │    │  Command Handler │   │
│  │  (passive)    │    │ (connected)  │    │  (Convex → BLE)  │   │
│  └──────┬───────┘    └──────┬───────┘    └────────┬─────────┘   │
│         │                   │                     │             │
│         │    ┌──────────────┴──────────────┐      │             │
│         │    │       UART Codec            │      │             │
│         │    │  • Frame sync (0xCA 0xFE)   │      │             │
│         │    │  • CRC-16-CCITT             │      │             │
│         │    │  • Request/Response headers  │      │             │
│         │    │  • Message serialization     │      │             │
│         │    └──────────────┬──────────────┘      │             │
│         │                   │                     │             │
│  ┌──────┴───────────────────┴─────────────────────┴──────┐      │
│  │                    Packet Decoder                      │      │
│  │  • Advertisement identity parsers                     │      │
│  │  • Probe advertising parser                           │      │
│  │  • Probe Status parser                                │      │
│  │  • Probe log/command response parser                  │      │
│  │  • Temperature conversion (raw → °C/°F)               │      │
│  │  • Bitfield unpacking (predictions, food safe, etc.)  │      │
│  └───────────────────────┬───────────────────────────────┘      │
│                          │ Typed domain objects                  │
│  ┌───────────────────────┴───────────────────────────────┐      │
│  │                   Session Manager                      │      │
│  │  • Tracks active cook sessions per probe              │      │
│  │  • Detects new sessions (session ID changes)          │      │
│  │  • Manages log backfill on connect/reconnect          │      │
│  └───────────────────────┬───────────────────────────────┘      │
│                          │                                      │
│  ┌───────────────────────┴───────────────────────────────┐      │
│  │                   Convex Sync Layer                    │      │
│  │  • Batches writes for efficiency                      │      │
│  │  • Polls commands table via HTTP                       │      │
│  │  • Handles offline/reconnect buffering                │      │
│  └───────────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────────┘
```

## Components

### BLE Scanner (passive)

Continuously scans for Combustion Inc advertising packets (vendor ID `0x09C7`). The scanner must classify advertisements by family and extract canonical identity accordingly:

- direct probe advertisements -> exact probe `productType + serialNumber`,
- node repeated-probe advertisements -> exact probe `productType + serialNumber` for the repeated probe,
- node self-advertisements -> exact node-family `productType + serialNumber`.

The scanner emits raw manufacturer data for downstream parsing, keys discovered devices by exact Combustion `productType + serialNumber`, and treats any BLE address as an ephemeral transport handle only. It runs independently of the node connection, serves as a fallback data source, and provides advertising-only data such as RSSI from the SBC's perspective.

### Node Gateway (connected)

Establishes and maintains a BLE connection to one MeatNet node. Subscribes to the UART TX characteristic for notifications. Receives Probe Status (`0x45`), Heartbeat (`0x49`), topology responses (`0x42`/`0x43`), and relevant probe command responses. On connect, reads Device Information characteristics needed to confirm or finalize node-family canonical identity, including `Serial Number String`. Handles reconnection with exponential backoff if the connection drops.

### UART Codec

Handles the low-level UART protocol:

- **Frame synchronization** — Finds `0xCA 0xFE` sync bytes in the byte stream
- **CRC-16-CCITT** — Validates inbound messages and computes CRC for outbound (polynomial `0x1021`, initial `0xFFFF`)
- **Header parsing** — Node request headers are 10 bytes (sync, CRC, message type, request ID, payload length). Node response headers are 15 bytes (adds response ID and success flag).
- **Message serialization** — Encodes outbound commands with proper headers, random request IDs, and CRC

### Packet Decoder

Pure functions that transform raw bytes into typed domain objects. All the bitfield unpacking lives here:

- **Raw temperatures** — 13-byte field → 8 individual readings via 13-bit extraction. Formula: `(raw * 0.05) - 20` for probes.
- **Mode/ID** — 1-byte → mode enum (Normal/Instant Read/Error), color enum, probe ID
- **Prediction status** — 7 bytes → state, mode, type, set point temp, heat start temp, seconds remaining, estimated core temp
- **Food safe data** — 10 bytes → mode, product category, serving type, threshold/Z/reference temps, D-value, target log reduction
- **Food safe status** — 8 bytes → safe/not safe/impossible state, achieved log reduction, seconds above threshold
- **Alarm status** — 2 bytes per alarm x 11 alarms x 2 (high/low) → set, tripped, alarming, alarm temperature

### Session Manager

Tracks cook sessions across probes. Each probe generates a random session ID when removed from the charger. When the session manager sees a new session ID for a known probe, it creates a new cook session record in Convex. Also manages log backfill: when a session is first detected or after reconnection, it reads the probe's log range and fetches any historical records the SBC missed.

#### Backfill Timestamp Reconstruction (default)

Backfilled log records do not include wall-clock time, so timestamps are reconstructed deterministically:

1. **Anchor selection** — Use the first live Probe Status packet (`0x45`) received after reconnect for that probe as the anchor: `(anchorSequence, anchorTimestampMs)`.
2. **Forward/backward fill** — For each backfilled log with sequence `s`, compute:
   `estimatedTimestampMs = anchorTimestampMs - ((anchorSequence - s) * samplePeriodMs)`
3. **No anchor fallback** — If no post-reconnect live packet exists yet, use reconnect time as temporary anchor and mark all derived points as `backfillEstimated`.
4. **Provenance** — Store `capturedAt` as ingest time and `timestampSource = backfillEstimated` for all reconstructed points.
5. **MVP stability rule** — Persist provenance (`timestampSource`, `timestampConfidence`) and do not run automatic timestamp rewrites.

### Network Health Monitor

Tracks mesh-level diagnostics from the connected node:

- **Heartbeat stream (`0x49`)** — Stores per-node heartbeat freshness, hop count, and connection RSSI details
- **Topology polling (`0x42`/`0x43`)** — Periodically requests node list + topology snapshots for historical network diagnostics
- **Health synthesis** — Computes derived status (healthy/degraded/offline) used by the web app network page and operational alerts

#### Health State Thresholds (default)

- **Topology poll interval:** every `15s`
- **Healthy:** last heartbeat age `<= 15s` and median RSSI `>= -75 dBm` and hop count `<= 2`
- **Degraded:** last heartbeat age `> 15s` and `<= 45s`, or median RSSI between `-90` and `-75 dBm`, or hop count `>= 3`
- **Offline:** no heartbeat for `> 45s`
- **State hysteresis:** require `2` consecutive evaluations before changing state to reduce flapping

### Command Handler

Polls the Convex commands table via HTTP for pending commands. When a new command is found:

1. Checks `expiresAt` — skips expired commands (marks as `failed` with error `"expired"`)
2. Acquires an execution lease (`pending` → `leased`) using compare-and-set semantics
3. Encodes as a UART request message with proper header and CRC
4. Sends via the node gateway's UART RX characteristic
5. Updates status to `sent` with `sentAt` timestamp
6. Waits for response on TX characteristic (matched by request ID)
7. Updates status to a terminal state (`succeeded` or `failed`) with `completedAt` timestamp

Supported commands (MVP): Set Prediction, Configure Food Safe, Reset Food Safe, Set Probe Alarms, Silence Alarms.

### Convex Sync Layer

Manages the connection to Convex:

- **Fixed-interval persistence** — Temperature readings are sampled before persistence at a fixed cadence (default `5s`, configurable to `1s`) to keep write volume predictable.
- **Command polling** — Polls the commands table via HTTP every 1-2 seconds for pending commands.
- **Durable local spool** — Appends outbound events to a local durable spool before Convex materialization and replays it on reconnect/startup.
- **Credential lifecycle (minimal)** — Uses one provisioned SBC service credential stored locally with strict file permissions, validates it at startup, and rotates/revokes only on explicit reprovision or suspected key leak.
- **Least privilege scope** — SBC credential is limited to required ingest and command-queue operations.

### Debug Server Access Policy (MVP)

- Default bind address is loopback-only (`127.0.0.1`).
- Optional LAN access requires explicit config enablement.
- When LAN mode is enabled, require a shared debug token.
- Debug endpoints are read-only by default.
- Debug server can be disabled entirely by configuration.
