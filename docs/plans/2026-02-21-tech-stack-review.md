# Tech Stack Review

## Overview

After completing the full system design (architecture, SBC internals, Convex schema, web app, testing, error handling), we critically reviewed every hardware and tech stack decision. This document captures the analysis and the decisions that changed.

## Decisions That Changed

### SBC Service Language: TypeScript → Rust

**Original decision:** TypeScript/Node.js for code sharing with the web app.

**Why it changed:** The "shared code" benefit was weaker than expected. Convex is the contract boundary between SBC and web app — they share a data model, not code. The SBC and web app never import each other's modules. Meanwhile, the BLE library situation in Node.js is a serious risk:

- **noble/@abandonware/noble** is effectively abandoned, uses native C++ addons that break with Node.js updates, bypasses BlueZ's D-Bus interface (causing conflicts), and has known issues with simultaneous scanning + GATT connections.
- **node-ble** is better (pure JS, D-Bus based) but less battle-tested.
- Neither compares to **bluer** (Rust) or **bleak** (Python) for BLE reliability.

**New decision:** Rust with bluer.

**Rationale:**
- bluer is a first-class BlueZ library, actively maintained, with excellent multi-connection support
- Rust's type system maps naturally to the Combustion protocol's packed bitfields and binary formats
- Minimal memory footprint (~5-10MB vs ~50-100MB for Node.js) — ideal for SBC
- Prior working code existed in this repo with bluer
- Convex HTTP API handles writes and polling for commands (1-2s polling latency is acceptable for device commands)

**Tradeoffs accepted:**
- Slower initial development than TypeScript
- Different language from the web app (mitigated by Convex being the shared boundary)
- No native Convex real-time subscriptions (mitigated by HTTP polling for commands)

### BLE Library: noble → bluer

Direct consequence of the language change. bluer talks to BlueZ via D-Bus, the correct and stable interface on Linux. It handles concurrent scanning and GATT connections reliably.

### Debug App: Standalone web server → Embedded in Rust service

**Original:** Separate Node.js/Next.js app on the SBC.

**New:** The Rust SBC service embeds a lightweight web server (axum) that serves a debug page on a local port. Static HTML/JS page with a WebSocket for live data streaming. No extra runtime dependency on the SBC.

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

### Repo Structure: Flat → Monorepo with workspace directories

**Original:** Single flat repo.

**New:** Monorepo with clear directory boundaries for each component. Necessary because Rust (Cargo) and TypeScript (npm) have different build systems.

```
meatnet-client/
├── docs/plans/         # Design documents
├── sbc-service/        # Rust (Cargo.toml)
│   ├── src/
│   └── tests/
├── web-app/            # Next.js + Convex
│   ├── convex/         # Convex schema + functions
│   ├── src/
│   └── package.json
├── sbc-service/debug/  # Debug UI static assets (embedded in Rust binary via axum)
├── external-docs/      # Combustion spec (gitignored)
└── test-fixtures/      # Captured BLE data (JSON fixtures)
```

## Decisions That Held

### Convex for Database and Sync

**Challenged on:** Time-series data volume and historical query performance.

**Analysis:** The math works out. A 12-hour cook with 4 probes generates ~86,400 temperature readings. Batched at 10 per mutation, that's ~8,640 mutations per cook — within Convex free tier limits. Annual storage at ~675MB is manageable.

Historical queries (loading 20,000+ readings for chart rendering) may need downsampling — return every Nth reading for zoomed-out views, full resolution when zoomed in. This is a display optimization, not a storage limitation.

**Decision:** Keep Convex. Its real-time sync for the live dashboard and command flow justifies using it for the time-series workload. Downsample on read for historical charts if needed.

### Next.js + Vercel for Web App

**Challenged on:** Whether a simpler SPA (React + Vite) would suffice.

**Analysis:** Convex has first-class Next.js support. Vercel deployment is trivial. The complexity difference between Next.js and a Vite SPA is minimal. Next.js gives better initial page load for the historical analysis pages.

**Decision:** Keep Next.js + Vercel. No change needed.

### Raspberry Pi for SBC Hardware

**Challenged on:** Whether ESP32 or other alternatives are better.

**Analysis:** ESP32 ($5) has BLE+WiFi but can only run C/MicroPython with very limited memory — can't run our full service stack (HTTP client, buffering, concurrent BLE). Other SBCs (Orange Pi, ODROID) work but have less community support for BLE/BlueZ.

**Decision:** Raspberry Pi. Pi Zero 2W (~$15) for minimum cost, Pi 4/5 for headroom. Best BlueZ/BLE ecosystem support on Linux.

### Node Gateway BLE Strategy

**Challenged on:** Whether direct probe connections or acting as a MeatNet node ourselves would be better.

**Analysis:** For MVP we only support probes. Direct probe connections would still complicate multi-probe coverage and connection-slot management. Acting as a MeatNet node ourselves is technically possible but massively complex for v1. The node gateway approach mirrors how the official app works and gives us everything we need through a single connection.

**Decision:** Keep node gateway + passive scan. No change.

## Updated Technology Stack

| Component | Technology | Changed? |
|-----------|-----------|----------|
| SBC Service | **Rust** | Yes (was TypeScript) |
| BLE Library | **bluer** | Yes (was noble) |
| HTTP Client | **reqwest** (Rust) | New |
| Debug Server | **axum** (embedded in SBC service) | Yes (was standalone) |
| Database & Sync | Convex | No |
| Web Framework | Next.js | No |
| Web Hosting | Vercel | No |
| SBC Hardware | Raspberry Pi | No |
| Repo Structure | **Monorepo with workspace dirs** | Yes (was flat) |
