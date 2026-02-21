# Web App Design

## Overview

A Next.js application hosted on Vercel that provides live cook monitoring, historical analysis, and device control. All data flows through Convex real-time subscriptions. The app has two primary modes: live monitoring during cooks and historical analysis after.

## Page Structure

```
/                        → Dashboard (active cooks or recent history)
/cook/[sessionId]        → Live cook view (real-time) or completed cook review
/history                 → Cook journal with search/filter
/compare                 → Side-by-side cook comparison
/devices                 → Device registry (read-only info)
/devices/[serialNumber]  → Device detail (info, firmware, link to active session)
/network                 → MeatNet mesh health and topology diagnostics
```

## Multi-Probe Navigation

A persistent session bar appears when there are active cooks. It is sticky at the top of every page and shows a small card per active session. Each card displays probe ID/color, current core temp, and prediction countdown. Clicking navigates to that session's live view.

```
┌────────────────────────────────────────────────────────────────┐
│ Probe #1: 165°F (32 min)  │  Probe #3: 203°F (Done!)  │
└────────────────────────────────────────────────────────────────┘
│                                                                │
│                    [Current Page Content]                       │
│                                                                │
```

This provides one-tap switching between probes from anywhere and an always-visible summary of all active cooks.

## Pages

### Dashboard (`/`)

The landing page adapts based on whether there are active cooks.

**Active cooks present:** Shows a card per active probe session with:

- Current core/surface/ambient temperatures (large, prominent)
- Mini temperature sparkline (last ~30 minutes)
- Prediction status: "Ready in 47 min" or "Warming up" or "Probe not inserted"
- Food safety state with progress indicator
- Battery status, probe color/ID
- Quick actions: set prediction target, silence alarms

**No active cooks:** Shows recent cook history cards with summary stats (peak temp, total time, final food safety state).

### Live Cook View (`/cook/[sessionId]` — active session)

The primary real-time monitoring screen. All data arrives via Convex subscriptions.

**Temperature Chart** — Full-width time-series chart:

- Core temperature (bold, primary line)
- Surface temperature
- Ambient temperature
- Individual T1-T8 sensors (toggleable)
- Prediction set point as a horizontal target line
- Estimated time to target as an annotation

**Prediction Panel:**

- State (warming → predicting → done)
- Mode (time to removal / removal and resting)
- Set point temperature
- Seconds/minutes remaining (countdown)
- Percentage complete (heat start → set point progress)
- Controls: change set point, change prediction mode

**Food Safety Panel:**

- State (not safe / safe / safety impossible)
- Mode (simplified / integrated)
- Product category
- Log reduction achieved (for integrated mode)
- Time above threshold

**Alarm Status** — Visual indicator for all 11 alarm channels (T1-T8, Core, Surface, Ambient), showing set/tripped/alarming state with configured temperatures. Controls to set or silence alarms.

**Cook Details Panel** — Expandable panel for editing cook metadata (see Cook Metadata section below). Starts collapsed with a prompt: "Add cook details."

### Completed Cook Review (`/cook/[sessionId]` — ended session)

Same layout as the live view, but in review mode:

- Full historical temperature chart (no live streaming)
- Prediction timeline showing state transitions
- Food safety timeline
- All metadata editable, plus post-cook fields (rating, post-cook notes)
- No device controls (cook is over)

### Cook History (`/history`)

A searchable, filterable journal of all completed cooks.

**List view** with each entry showing:

- Date, duration, device used
- Title, protein type, user notes
- Peak core temperature, final food safety state
- Thumbnail temperature curve
- Inline edit affordance for quick tagging/titling without opening each cook

**Filters:** Date range, device, protein type, cook method, food safety outcome, rating, tags.

### Cook Comparison (`/compare`)

Select 2-4 cooks and overlay their temperature curves on a single chart. Time-aligned from cook start.

Shows a comparison table with key metrics side by side: total cook time, time to prediction, peak temps, food safety time.

### Device Management (`/devices`)

**Device list** — All known devices with last seen time, battery status, firmware version.

**Device detail page** — Read-only device information: serial number, product type, firmware/hardware revision, last seen. Links to active cook session if one exists for this device. No device controls on this page — all control happens in the live cook view.

### Network Diagnostics (`/network`)

Operational view of MeatNet mesh health. This page is read-only and intended for troubleshooting connection quality.

- Current gateway node and connection status
- Heartbeat freshness per discovered node
- Topology graph with inbound/outbound links and RSSI color coding
- Derived health state (`healthy`, `degraded`, `offline`) per node
- Recent probe advertising RSSI trend from the SBC scanner as a local signal-quality overlay

## Cook Metadata

Since sessions are auto-created when the probe leaves the charger, the cook record starts with no metadata. Users can enrich it at any point during or after the cook.

### Metadata Fields

| Field | Type | Description |
|-------|------|-------------|
| `title` | string? | User-given cook name (e.g., "Sunday Brisket") |
| `protein` | string? | Protein type (beef, pork, poultry, fish, lamb, etc.) |
| `cut` | string? | Specific cut (e.g., "packer brisket", "pork butt") |
| `weightGrams` | number? | Weight in grams (UI converts to/from lb/kg) |
| `cookMethod` | string? | Smoke, grill, oven, sous vide, etc. |
| `notes` | string? | Ongoing observations (e.g., "wrapped at 165F", "spritzed every hour") |
| `postCookNotes` | string? | Post-cook reflections |
| `rating` | number? | 1-5 rating |
| `tags` | string[]? | User-defined tags for filtering |

All fields are optional and auto-save on edit via Convex mutations.

### Editing Context

- **During a cook:** Cook Details panel in the live view. All fields except `postCookNotes` and `rating`.
- **After a cook:** Completed cook review page. All fields including `postCookNotes` and `rating`.
- **From history list:** Inline quick-edit for title, protein, tags, and rating.

## Device Control

Device control is only available during active cooks, within the live cook view. This keeps controls contextually relevant and prevents sending commands to devices that aren't actively cooking.

### Available Controls by Device Type

**Probe (MVP):**

- Set prediction (target temp, mode: time to removal / removal and resting)
- Configure food safety (mode, product, serving)
- Reset food safety
- Set high/low alarms (per sensor channel)
- Silence alarms

All control actions write to the `deviceCommands` Convex table and display the four-step acknowledgement progress inline (queued → SBC received → sent to device → device confirmed).
