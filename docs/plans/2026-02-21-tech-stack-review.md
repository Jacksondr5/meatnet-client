# Tech Stack Decisions

## Overview

This document captures the current technology choices for the MeatNet companion system and the rationale for each decision.

## Current Decisions

### SBC Service: Rust with `bluer`

The SBC runtime is a Rust service using `bluer` for BLE communication over BlueZ D-Bus.

Rationale:

- BLE reliability and strong multi-connection behavior on Linux
- Good fit for binary protocol parsing and bitfield-heavy payloads
- Lower runtime footprint on Raspberry Pi hardware

### Debug Interface: Embedded `axum` server in SBC service

The SBC service includes a lightweight debug web server.

```
Rust SBC Service
  ├── BLE Scanner
  ├── Node Gateway
  ├── Packet Decoder
  ├── Convex Sync
  └── Debug Server (axum)
        ├── GET /debug → static HTML/JS
        └── WS  /ws   → live decoded data

Access: http://sbc-ip:3001/debug
```

Rationale:

- No separate runtime/process required on SBC
- Direct in-process visibility into raw and decoded protocol data
- Lower operational complexity during development/debugging

### Data and Sync: Convex

Convex is the system-of-record and sync layer between SBC and web app.

Rationale:

- Real-time subscription model fits live cook monitoring
- Straightforward command queue flow between web UI and SBC
- Sufficient scale for MVP with write batching and read-time downsampling for long histories

### Web App: Next.js on Vercel

Rationale:

- Good integration with Convex
- Fast deployment and hosting workflow
- Supports live dashboards and historical analysis pages in a single app

### SBC Hardware: Raspberry Pi

Rationale:

- Mature Linux + BlueZ ecosystem
- Practical CPU/memory profile for BLE, buffering, and protocol processing
- Broad community support and hardware availability

### Repository Layout: Monorepo with component directories

```
meatnet-client/
├── docs/plans/
├── sbc-service/
├── web-app/
├── sbc-service/debug/
├── external-docs/
└── test-fixtures/
```

Rationale:

- Clear separation between Rust and TypeScript toolchains
- Better boundaries for testing, CI, and deployment ownership

## Operational Notes

- Historical chart queries should use downsampling for large ranges and full resolution when zoomed in.
- Reliability semantics (durability, idempotency, command leasing, degraded states) are defined in [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md).
