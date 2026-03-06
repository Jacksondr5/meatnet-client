# MeatNet Companion — Implementation Phases

## Overview

This document outlines the implementation phases for the MeatNet Companion system. Each phase produces a concrete, testable milestone. Phases are ordered by dependency — later phases build on earlier ones.

**Design documents:** `docs/plans/2026-02-21-*.md`, `docs/plans/2026-03-06-reliability-contract-design.md`
**BLE protocol specs:** `external-docs/`
**Detailed phase plans:** `docs/plans/phases/phase-N-*.md`

---

## Phase 1: BLE Foundation
**Milestone: "I can connect to Combustion devices and receive raw bytes"**

- Rust project scaffold (`sbc-service/` — Cargo.toml, bluer, tokio, axum, reqwest)
- BLE passive scanning with bluer — discover devices by vendor ID `0x09C7`
- Node discovery — identify nodes vs probes from product type byte
- GATT connection to one node — connect, discover UART service, subscribe to TX notifications
- Receive raw UART bytes and log to console
- Reconnection with exponential backoff on disconnect
- **Requires Raspberry Pi + Combustion hardware from day one**

**Plan:** [phase-1-ble-foundation.md](./phase-1-ble-foundation.md)

---

## Phase 2: Raw Data Capture + Debug Server
**Milestone: "I can capture raw BLE data as test fixtures and inspect it live"**

- JSON fixture file format (timestamped, source-tagged, scenario metadata — per the testing strategy doc)
- Capture tool — CLI mode that writes raw bytes to fixture files, organized by scenario
- Embedded axum debug server — `GET /debug` static HTML page, `WS /ws` live raw hex stream
- Debug UI: live scrolling view of raw bytes with source labels (advertising vs UART TX) and timestamps
- Capture the key scenarios: probe lifecycle, prediction progression, multiple probes, heartbeats, topology
- `test-fixtures/` directory with organized captured data

**Plan:** [phase-2-raw-data-capture.md](./phase-2-raw-data-capture.md)

---

## Phase 3: Protocol Foundation
**Milestone: "I can decode any Combustion BLE packet correctly"**

- Domain types — all Rust enums/structs mapping to the BLE protocol (temperatures, modes, predictions, food safe, alarms, etc.)
- Packet decoder — pure functions, TDD against Phase 2 fixtures:
  - Raw temperature (13-bit extraction from 13 bytes)
  - Mode/ID, battery/virtual sensors, overheating
  - Prediction status (7-byte bitfield)
  - Food safe data (10-byte) and status (8-byte)
  - Alarm status (22 x uint16)
  - Full advertising packet parser (24-byte manufacturer data)
  - Full Probe Status parser (all fields from `0x45`)
  - Read Logs response parser (sequence + temps + prediction log)
- UART codec — frame sync (`0xCAFE`), CRC-16-CCITT, node request headers (10-byte), node response headers (15-byte), message serialization
- Enhance debug server: add parsed results alongside raw bytes (side-by-side view)
- **No hardware needed — pure Rust + tests against captured fixtures**

---

## Phase 4: Convex Schema + API Layer
**Milestone: "Database is ready to receive and serve cook data"**

- `web-app/` project setup (Next.js + Convex)
- Convex schema: `devices`, `cookSessions`, `temperatureReadings`, `predictionSnapshots`, `foodSafetySnapshots`, `deviceCommands`, `networkTopology`, `heartbeats`
- Mutations: ingest temperature readings (fixed-interval cadence), create sessions, record prediction/food safety snapshots, register devices
- Queries: active sessions, session readings, device list, command queue
- Convex function tests
- **Can be developed in parallel with Phase 3**

---

## Phase 5: Auth + Provisioning Foundation
**Milestone: "Web users and SBC machine identity are both authenticated and owner-scoped"**

- Clerk + Convex auth integration for web app user sessions
- Define single-user household ownership model (`ownerId`) and enforce owner-scoped reads/writes
- Provision one SBC machine principal (M2M) per deployment
- SBC credential storage and startup validation path
- Credential rotation/revocation workflow (manual reprovision + recovery UX)
- Basic auth observability: token/lease failures surfaced in logs and UI status

---

## Phase 6: SBC Application Core
**Milestone: "Data flows from probe → SBC → Convex automatically"**

- Session manager — detect new sessions from session ID changes, create/end sessions in Convex
- Convex sync layer — reqwest HTTP client, fixed-interval persistence, command polling
- Main application loop wiring: BLE → decode → session manager → Convex sync
- SBC startup: query Convex for active sessions, reconnect, resume
- Durable local spooling for outbound Convex events
- **First end-to-end data flow from hardware to cloud**

---

## Phase 7: Web App MVP
**Milestone: "I can see live cook data in a web browser"**

- Convex client integration with real-time subscriptions
- Dashboard page — active cook cards with live temps, prediction, food safety
- Live cook view — temperature chart (core/surface/ambient + T1-T8 toggleable), prediction panel, food safety panel
- Active session bar (sticky, multi-probe navigation)
- Device list page (read-only)
- **First full end-to-end demo**

---

## Phase 8: Commands + Device Control
**Milestone: "I can control probes from the web"**

- Command handler in SBC — poll Convex, encode UART commands, send via node, match responses
- Command encoding for: Set Prediction, Configure Food Safe, Reset Food Safe, Set Alarms, Silence Alarms
- Expiration/TTL handling
- Web UI: prediction controls, food safety controls, alarm controls
- Lease-based acknowledgement display (`pending` → `leased` → `sent` → terminal)

---

## Phase 9: Log Backfill + Resilience
**Milestone: "System recovers gracefully from any failure"**

- Log backfill — Read Logs (`0x04`) request/response, gap detection, sequence-based backfill
- Timestamp reconstruction (anchor to first post-reconnect live packet, derive historical times)
- `timestampSource` + `timestampConfidence` provenance tracking
- Durable spool replay on reconnect/startup
- SBC crash recovery (query Convex for active sessions on startup)

---

## Phase 10: Network Health
**Milestone: "Full mesh visibility and developer tooling"**

- Heartbeat (`0x49`) parsing and storage
- Topology polling (`0x42`/`0x43`) — node list + topology snapshots
- Health state machine (healthy/degraded/offline with hysteresis)
- Network diagnostics web page
- Debug server enhanced with parsed data, session state, Convex sync status

---

## Phase 11: Web App Polish
**Milestone: "Feature-complete web application"**

- Cook history page with search/filter
- Cook comparison (overlay 2-4 temperature curves)
- Cook metadata editing (during and after cooks)
- Completed cook review mode
- Full alarm UI (all 11 channels, set/tripped/alarming)
- Responsive design pass

---

## Parallelization Notes

- **Phases 3 and 4 can run in parallel** (protocol decoding and Convex schema are independent)
- **Phase 5 should complete before Phases 6-8** so SBC ingest/commands and web actions are auth-scoped from day one
- The debug server grows incrementally across phases:
  - Phase 1: n/a
  - Phase 2: raw hex display
  - Phase 3: raw + parsed side-by-side
  - Phase 6+: session state, Convex sync status
- The first time you see data in a browser is after Phase 7 — Phases 1-6 are infrastructure
- Phases 9-11 are "polish and completeness" — the core system works after Phase 8
