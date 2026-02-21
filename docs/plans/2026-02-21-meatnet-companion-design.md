# MeatNet Companion System Design

## Overview

A companion system for the Combustion Inc MeatNet ecosystem that captures full cook data, stores it for analysis, and provides a web-based UI for live monitoring, historical analysis, and device control.

## Devices Supported

MVP scope:

- **Predictive Probe** — 8-sensor thermometer with on-device cook prediction and food safety calculations
- **MeatNet Node (gateway role)** — Repeater/Display/Booster used only as a bridge to probes on the MeatNet mesh

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        GRILL SIDE                           │
│                                                             │
│  [Probes] ───────── MeatNet ── [Node/Repeater]              │
│                            BLE Connection                    │
│                                  │                           │
│                          ┌───────┴────────┐                  │
│                          │   SBC Service   │                  │
│                          │    (Rust)       │                  │
│                          │                 │                  │
│                          │ • BLE Manager   │                  │
│                          │ • Packet Decoder │                 │
│                          │ • UART Protocol │                  │
│                          │ • Convex Client │                  │
│                          └───────┬────────┘                  │
└──────────────────────────────────┼───────────────────────────┘
                                   │ HTTPS (outbound only)
                                   │
┌──────────────────────────────────┼───────────────────────────┐
│                          CLOUD SIDE                          │
│                                  │                           │
│                          ┌───────┴────────┐                  │
│                          │     Convex      │                  │
│                          │                 │                  │
│                          │ • Cook Sessions │                  │
│                          │ • Temperature   │                  │
│                          │   Time Series   │                  │
│                          │ • Predictions   │                  │
│                          │ • Device State  │                  │
│                          │ • Commands      │                  │
│                          │ • Mesh Health   │                  │
│                          │ • Command Queue │                  │
│                          └───────┬────────┘                  │
│                                  │ Real-time sync            │
│                          ┌───────┴────────┐                  │
│                          │  Next.js App   │                  │
│                          │  (Vercel)      │                  │
│                          │                │                  │
│                          │ • Live Dashboard│                 │
│                          │ • Cook History  │                  │
│                          │ • Device Control│                  │
│                          │ • Analytics     │                  │
│                          └────────────────┘                  │
└──────────────────────────────────────────────────────────────┘
```

### Components

**SBC Service (Rust)** — Runs on a Raspberry Pi near the grill. Connects to a MeatNet node as a gateway to probes on the mesh using bluer (BlueZ D-Bus interface). Decodes probe BLE protocol data (advertising packets, Probe Status notifications, UART messages) and network-health messages (heartbeats and topology). Pushes data to Convex via HTTP API. Polls Convex for commands and relays them to probes via the node's UART service. Also passively scans BLE advertising packets for redundancy. Includes an embedded debug web server (axum) for live protocol inspection.

**Convex** — Real-time database and sync layer. Stores all cook data, device state, and pending commands. Handles bidirectional communication between SBC and web app. The web app uses Convex's built-in real-time subscriptions for live data. The SBC pushes data via Convex HTTP API and polls for pending commands.

**Next.js Web App (Vercel)** — User-facing interface. Subscribes to Convex for live data during cooks. Provides historical analysis, cook comparison views, and a read-only network diagnostics page for mesh-health visibility. Writes probe control commands to Convex.

### BLE Strategy: Node Gateway + Passive Scan

The SBC connects to one MeatNet node (e.g., repeater or display) as its gateway to the probe network. This mirrors how the official Combustion app works for probe data.

Passive BLE advertising scanning runs in parallel for redundancy — if the node connection drops, advertising data provides basic temperature readings while the connection is re-established.

The connected node also provides mesh-health visibility through heartbeat and topology messages, so we can monitor link quality and relay health while keeping cook/session features probe-focused.

```
  [Probe 1] ----\
  [Probe 2] -----[Node/Repeater] <===BLE===> [SBC] ---internet---> [Convex]
  [Probe 3] ----/       |                                            |
                        |                                            v
                   (MeatNet mesh)                              [Next.js on Vercel]

  SBC also passively scans advertising
  packets from probes in range.
```

### Data Capture Strategy

Probe-focused capture:

- **Probe Status notifications** — Real-time stream of all 8 temperatures, predictions, food safety, alarms, battery, and mode for every probe on the network
- **UART log backfill** — Read historical logs from probe memory to fill gaps (e.g., after SBC reboot or for cooks that started before the SBC connected)
- **Advertising packets** — Passive scan for redundancy and additional probe RSSI data
- **Mesh health messages** — Heartbeats (`0x49`) and topology snapshots from the gateway node for network diagnostics

### Command Flow (Web UI to Devices)

Bidirectional communication flows through Convex:

1. Web UI writes a command record to a Convex commands table (e.g., "set prediction to 203F on probe serial X")
2. SBC polls the commands table via HTTP and picks up new pending commands
3. SBC encodes the command per the UART protocol and sends it to the device via the node gateway
4. SBC writes the command result (success/failure) back to Convex
5. Web UI sees the result via its Convex subscription

### Technology Stack

| Component | Technology |
|-----------|-----------|
| SBC Service | Rust |
| BLE Library | bluer |
| HTTP Client | reqwest |
| Debug Server | axum (embedded in SBC service) |
| Database & Sync | Convex |
| Web Framework | Next.js |
| Web Hosting | Vercel |
| SBC Hardware | Raspberry Pi |
| Repo Structure | Monorepo with workspace dirs |

> **Note:** The SBC service was changed from TypeScript to Rust after the tech stack review.
> See [Tech Stack Review](./2026-02-21-tech-stack-review.md) for the full analysis.

### Design Goals

- **Full cook data capture** — Every temperature reading, prediction update, and food safety state change recorded for every cook
- **Live monitoring** — Real-time dashboard accessible from any device on any network (not just local)
- **Historical analysis** — Compare cooks over time, overlay temperature curves, track improvement
- **Probe control during active cooks** — Prediction, food safety, and alarms from the web UI
- **Resilient** — Passive advertising scan as fallback, log backfill for gap recovery
- **Mesh observability** — Track heartbeat freshness and link RSSI to diagnose network issues

## SBC Service Internals

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
- **Alarm status** — 2 bytes per alarm × 11 alarms × 2 (high/low) → set, tripped, alarming, alarm temperature

### Session Manager

Tracks cook sessions across probes. Each probe generates a random session ID when removed from the charger. When the session manager sees a new session ID for a known probe, it creates a new cook session record in Convex. Also manages log backfill: when a session is first detected or after reconnection, it reads the probe's log range and fetches any historical records the SBC missed.

### Command Handler

Polls the Convex commands table via HTTP for pending commands. When a new command is found:

1. Encodes it as a UART request message with proper header and CRC
2. Sends it via the node gateway's UART RX characteristic
3. Waits for the response on the TX characteristic (matched by request ID)
4. Updates the command record in Convex with success/failure

Supported commands (MVP): Set Prediction, Configure Food Safe, Reset Food Safe, Set Probe Alarms, Silence Alarms.

### Convex Sync Layer

Manages the connection to Convex:

- **Write batching** — Temperature readings arrive every few seconds per probe across multiple probes. Batches writes to avoid excessive Convex mutations.
- **Command polling** — Polls the commands table via HTTP every 1-2 seconds for pending commands.
- **Offline buffering** — If internet connectivity drops, buffers data locally and flushes when reconnected to avoid data loss during cooks.
