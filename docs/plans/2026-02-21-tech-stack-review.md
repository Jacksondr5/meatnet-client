# Tech Stack Decisions

## Overview

This document captures the current technology choices for the MeatNet companion system and the rationale for each decision.

## Current Decisions

### SBC Service BLE Library: pending validation

The final BLE library choice for the SBC runtime is not locked yet.

Rationale:

- The project goal is MacBook-based development plus Raspberry Pi deployment.
- A single application-level BLE implementation is preferable if it satisfies the MeatNet requirements on both platforms.
- A Linux-native backend is still an option if cross-platform validation fails.

### BLE Transport Boundary: shared abstraction with dual backends

The BLE-facing code should be split into:

- a transport-neutral interface for scanning, connecting, reading, writing, and notifications,
- a candidate cross-platform backend for MacBook and Raspberry Pi validation,
- an optional Linux-native backend only if production needs demand it.

Rationale:

- Preserves the option of one application-level BLE codebase
- Allows macOS development and fixture capture from a MacBook Pro
- Prevents protocol parsing, session logic, and sync code from becoming tied to one BLE library
- Gives us a clean fallback if Raspberry Pi production needs diverge from MacBook development needs

Design details are defined in [2026-03-07-ble-transport-abstraction-design.md](./2026-03-07-ble-transport-abstraction-design.md).

Critical guardrail:

- If real hardware validation disproves the assumptions in the transport decision framework, implementation must stop and the user must be alerted to a possible architectural problem before work continues.
- Canonical device identity must be the exact Combustion `productType + serialNumber`, with the serial normalized according to the advertisement family or GATT Device Information data. BLE addresses and peripheral handles are transport-only and must not be used as durable identifiers.

### SBC Runtime Packaging: OCI Container

The SBC service is packaged and shipped as a container image for deployment consistency across development Linux machines and Raspberry Pi targets.

Rationale:

- Consistent runtime environment across amd64 and arm64
- Faster iteration loop for shipping/test cycles
- Simpler reproducibility for dependency and config behavior

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
- Sufficient scale for MVP with fixed-interval persistence (default 5s cadence) for long histories

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

- Historical chart resolution is controlled at ingest using fixed-interval persistence (default 5s, configurable to 1s).
- Reliability semantics (durability, idempotency, command leasing, degraded states) are defined in [2026-03-06-reliability-contract-design.md](./2026-03-06-reliability-contract-design.md).
- Container runtime details (process manager, update strategy, watchdog integration) are defined by operational readiness design work.
