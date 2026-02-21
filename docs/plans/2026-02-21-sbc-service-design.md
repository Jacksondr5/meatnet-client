# SBC Service Design

## Overview

The SBC service is a Rust process that runs on a Raspberry Pi near the grill. It bridges the BLE protocol world with the Convex cloud backend using bluer for BLE communication and reqwest for Convex HTTP API calls.

> **Note:** Originally designed as TypeScript/Node.js. Changed to Rust after the tech stack review
> for BLE reliability (bluer vs noble), binary parsing ergonomics, and minimal memory footprint.
> See [Tech Stack Review](./2026-02-21-tech-stack-review.md) for the full analysis.

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
│  │  • Advertising packet parser                          │      │
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

Continuously scans for Combustion Inc advertising packets (vendor ID `0x09C7`). For MVP, parses probe-format manufacturer data (temperature/mode/battery) and ignores non-probe payloads. Runs independently of the node connection. Serves as a fallback data source and provides advertising-only data (like RSSI from the SBC's perspective) that isn't available through the node.

### Node Gateway (connected)

Establishes and maintains a BLE connection to one MeatNet node. Subscribes to the UART TX characteristic for notifications. Receives Probe Status (`0x45`), Heartbeat (`0x49`), topology responses (`0x42`/`0x43`), and relevant probe command responses. Handles reconnection with exponential backoff if the connection drops.

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
5. **Stability rule** — Do not rewrite timestamps once persisted unless an explicit repair job is run.

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
2. Updates command status to `received` with `receivedAt` timestamp
3. Encodes as a UART request message with proper header and CRC
4. Sends via the node gateway's UART RX characteristic
5. Updates status to `sent` with `sentAt` timestamp
6. Waits for response on TX characteristic (matched by request ID)
7. Updates status to `success` or `failed` with `completedAt` timestamp

Supported commands (MVP): Set Prediction, Configure Food Safe, Reset Food Safe, Set Probe Alarms, Silence Alarms.

### Convex Sync Layer

Manages the connection to Convex:

- **Write batching** — Temperature readings arrive every few seconds per probe across multiple probes. Batches writes to avoid excessive Convex mutations.
- **Command polling** — Polls the commands table via HTTP every 1-2 seconds for pending commands.
- **Offline buffering** — If internet connectivity drops, buffers data locally and flushes when reconnected to avoid data loss during cooks.
