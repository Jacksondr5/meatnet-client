# Testing Strategy Design

## Overview

Testing for the MeatNet companion system is built on a foundation of real-world data capture. Before writing parsing code, we capture raw BLE bytes from live devices to create test fixtures. Automated tests then validate parsing logic against these fixtures. An embedded debug web server in the Rust SBC service provides live visibility into the raw-to-parsed pipeline during development.

## 1. Live Data Capture

### Capture Tool

A lightweight script that records raw BLE bytes from real Combustion Inc devices. This produces the test fixture files that unit tests run against.

### What to Capture

- **Raw advertising packets** — Full 24-byte probe manufacturer data (including Instant Read and normal modes)
- **UART messages** — Raw bytes from the TX characteristic for probe-focused messages: Probe Status (`0x45`), Heartbeat (`0x49`), topology messages (`0x42`/`0x43`), log reads (`0x04`), and probe command responses
- **Scenario sequences** — Ordered captures of specific real-world events

### Scenarios to Capture

| Scenario | What it produces | Why it matters |
|----------|-----------------|---------------|
| Probe removed from charger | New session ID in advertising/status | Session detection logic |
| Probe inserted into food | Mode change, virtual sensor assignment changes | Mode/sensor parsing |
| Prediction set and progressing | Prediction state transitions (not inserted → inserted → warming → predicting → done) | Prediction state machine |
| Food safety program running | Food safe state transitions | Food safety parsing |
| Probe returned to charger | Session ends, advertising stops | Session end detection |
| Multiple probes on network | Interleaved advertising, multiple Probe Status messages | Multi-probe handling |
| Device going out of range | Missing Probe Status and fallback to advertising | Disconnect detection |
| Node power cycle | Reconnection sequence and probe log backfill | Reconnection/backfill logic |
| Mesh link degradation | Heartbeats continue with worse RSSI and rising hop count | Network health state derivation |
| Node drop from mesh | Missing heartbeat for one node while others continue | Per-node offline detection |
| Alarms triggering | Alarm status transitions (set → tripped → alarming → silenced) | Alarm parsing |
| Instant Read mode | Mode change, different advertising interval, T1-only temps | Mode-specific parsing |

### Capture Format

Each capture is a JSON file with timestamped raw byte arrays and metadata:

```json
{
  "scenario": "probe-prediction-lifecycle",
  "description": "Probe #1 set to 95C removal, full cook until prediction done",
  "devices": ["probe:AABBCCDD"],
  "captures": [
    {
      "timestamp": 1708531200000,
      "source": "advertising",
      "rawBytes": "c709010aabbccdd...",
      "note": "probe just removed from charger"
    },
    {
      "timestamp": 1708531201000,
      "source": "uart_tx",
      "messageType": "0x45",
      "rawBytes": "cafe...",
      "note": "first probe status notification"
    }
  ]
}
```

## 2. Automated Testing

### SBC Service

Three layers of tests, ordered by importance:

#### Unit Tests: Packet Decoder (highest priority)

Pure function tests using captured fixtures. These are the most important tests in the project.

- Given 13 raw bytes, assert 8 correct temperatures
- Given 7-byte prediction status, assert correct state/mode/set point/seconds remaining
- Given a UART frame, assert correct CRC, message type, and payload extraction
- Given 10-byte food safe data, assert correct mode/product/serving/threshold values
- Edge cases: max/min temperatures, all sensors overheating, instant read mode (only T1 populated), battery low, all alarms tripped

These tests are fast, deterministic, and require no mocking.

#### Unit Tests: UART Codec

Test frame assembly and parsing:

- CRC-16-CCITT computation against known values
- Request header serialization (10-byte node format with request ID)
- Response header parsing (15-byte node format with response ID and success flag)
- Frame sync detection in a byte stream with noise/partial frames
- Round-trip: serialize a command → parse the bytes → assert they match
- Malformed frame handling: bad CRC, truncated frames, missing sync bytes

#### Integration Tests: Session Manager and Command Handler

These need mocked Convex and BLE layers:

- **Session Manager:** Feed a sequence of Probe Status messages with changing session IDs → assert correct session creation/ending in Convex
- **Command Handler:** Write a command to mock Convex → assert correct UART bytes sent → feed back a mock response → assert Convex updated with success/failure
- **Backfill logic:** Simulate reconnection with a gap in sequence numbers → assert correct log read requests
- **Command expiration:** Write an expired command → assert it is marked failed with "expired" error
- **Network health monitor:** Feed heartbeat/topology sequences with changing RSSI and missing nodes → assert expected `healthy/degraded/offline` state transitions

#### Not Tested Directly

BLE Scanner and Node Gateway are thin wrappers around the BLE library. Testing them requires real hardware. Instead, test the layers they feed into (decoder, session manager) with captured data.

### Web App

Two layers of tests:

#### Component Tests

Test key interactive components with mock Convex data:

- Temperature chart renders correctly with sample time-series data
- Cook details panel saves metadata on edit
- Command controls show acknowledgement progress through all four states (pending → received → sent → success/failed)
- Active session bar updates when sessions start/end
- Cook history filters produce correct query parameters

#### Convex Function Tests

Using Convex's built-in testing framework:

- **Mutations:** Creating sessions, writing temperature readings, updating command status
- **Queries:** Fetching active sessions, historical session queries with filters, temperature data for a time range, device list
- **Edge cases:** Concurrent writes from multiple probes, command expiration logic, session end detection

### What We Deliberately Skip

- **E2E browser tests** — High maintenance, low value at this stage
- **BLE integration tests** — Require real hardware, covered by capture-based unit tests
- **Visual regression tests** — Premature for a new app

## 3. Debug App

A debug web server embedded in the Rust SBC service (via axum). It uses the same in-process packet decoder and UART codec code and shows the full parsing pipeline in real time.

### Purpose

Provides live visibility into raw BLE bytes alongside parsed results during development. Invaluable for catching parsing bugs and understanding device behavior.

### Architecture

- Embedded in the Rust SBC service binary (not a separate process)
- Serves a static HTML/JS debug page on a local port (e.g., `http://sbc-hostname:3001/debug`)
- Has direct in-process access to the BLE scanner, node gateway, and packet decoder
- Streams decoded data to the browser via WebSocket
- No Convex dependency — works even before Convex is set up

### Display

Side-by-side view of raw bytes and parsed results for every BLE packet:

```
┌─────────────────────────────────────┬──────────────────────────────────┐
│ Raw Bytes                           │ Parsed Result                    │
├─────────────────────────────────────┼──────────────────────────────────┤
│ Source: UART TX (0x45 Probe Status) │ Probe Serial: AABBCCDD          │
│ c7 09 01 aa bb cc dd ...            │ T1: 72.3°C  T2: 68.1°C         │
│ Timestamp: 14:32:01.234             │ T3: 65.0°C  T4: 58.9°C         │
│                                     │ T5: 45.2°C  T6: 38.1°C         │
│ CRC: 0x3A7F (valid)                │ T7: 32.0°C  T8: 28.5°C         │
│ Request ID: 0x12345678              │ Mode: Normal  Color: Yellow     │
│ Payload: 24 bytes                   │ Core: T1  Surface: T4           │
│                                     │ Ambient: T7                     │
│                                     │ Prediction: Predicting          │
│                                     │ Set Point: 95.0°C               │
│                                     │ ETA: 2847 seconds               │
│                                     │ Food Safe: Not Safe             │
├─────────────────────────────────────┼──────────────────────────────────┤
│ Source: Advertising (Probe)         │ Serial: AABBCCDD                │
│ c7 09 01 aa bb cc dd 8f 2a ...      │ T1: 72.3°C                      │
│ Timestamp: 14:32:01.312             │ Mode: Normal  ID: 1             │
│                                     │ Battery: OK                     │
└─────────────────────────────────────┴──────────────────────────────────┘
```

### Lifecycle

Used heavily during early development for protocol validation. Becomes an occasional debugging tool once parsing is stable. Always available whenever the SBC service is running — no separate startup needed.
