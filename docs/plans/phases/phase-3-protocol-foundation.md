# Phase 3: Protocol Foundation Implementation Plan

**Goal:** Decode every field in the Combustion BLE protocol that we need for discovery and runtime operation — advertisement identity payloads, probe-format advertising data, Probe Status messages, log responses, and UART frames — using pure functions tested against spec-derived data and Phase 2 captured fixtures.

**Architecture:** A `protocol/` module containing pure parsing functions with zero BLE or I/O dependencies. Every parser takes a byte slice and returns a typed Rust struct. All code is TDD: write test with known bytes → verify fail → implement parser → verify pass. The debug server is enhanced to show parsed results alongside raw bytes.

**Tech Stack:** No new dependencies. Pure Rust with existing serde for debug serialization.

**Prerequisite:** Phase 2 complete — captured fixture files exist in `test-fixtures/`.

**Reference docs:**

- `external-docs/probe_ble_specification.rst` — Probe packet formats, field layouts, formulas
- `external-docs/meatnet_node_ble_specification.rst` — Node UART headers, message formats, Probe Status (0x45) layout
- `docs/plans/2026-02-21-sbc-service-design.md` — Packet decoder component description

**Key formulas from specs:**

- Temperature: `(raw_13bit * 0.05) - 20` °C
- Prediction set point: `raw_10bit * 0.1` °C
- Heat start temp: `raw_10bit * 0.1` °C
- Estimated core: `(raw_11bit * 0.1) - 20` °C
- Food safe decimal (13-bit): `raw * 0.05`
- Log reduction (8-bit): `raw * 0.1`
- Alarm temp: `(raw_13bit * 0.1) - 20` °C
- CRC-16-CCITT: polynomial `0x1021`, initial `0xFFFF`

---

## Project Structure (new files in this phase)

```
sbc-service/src/
├── protocol/
│   ├── mod.rs              # Module declarations, re-exports
│   ├── types.rs            # All domain enums and structs
│   ├── bits.rs             # Bit extraction from byte slices
│   ├── temperature.rs      # Raw temperature parsing (13-byte → 8 temps)
│   ├── mode_id.rs          # Mode/ID, battery/virtual sensors, overheating
│   ├── prediction.rs       # Prediction status + prediction log
│   ├── food_safe.rs        # Food safe data + status
│   ├── alarm.rs            # Alarm status arrays
│   ├── advertising.rs      # Advertisement identity + probe advertising parsers
│   ├── crc.rs              # CRC-16-CCITT
│   ├── uart.rs             # UART frame parser + serializer
│   └── probe_status.rs     # Probe Status (0x45) + Read Logs (0x04)
```

---

## Task 1: Protocol Scaffold + Bit Extraction + Temperature Parsing

**Files:**

- Create: `sbc-service/src/protocol/mod.rs`
- Create: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/bits.rs`
- Create: `sbc-service/src/protocol/temperature.rs`
- Modify: `sbc-service/src/main.rs` (add `mod protocol;`)

**Step 1: Create domain types**

```rust
// sbc-service/src/protocol/types.rs
use serde::Serialize;

/// 8 temperature readings from the probe sensors, in degrees Celsius.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeTemperatures {
    /// Temperatures for sensors T1 through T8, in °C.
    /// Range: -20 to 369°C. Resolution: 0.05°C.
    pub values: [f64; 8],
}
```

**Step 2: Write bit extraction tests**

```rust
// sbc-service/src/protocol/bits.rs

/// Extract `num_bits` bits starting at 0-indexed bit position `start_bit`
/// from a byte slice. Bits are numbered LSB-first within each byte:
/// bit 0 = byte[0] bit 0, bit 8 = byte[1] bit 0, etc.
///
/// This matches the Combustion BLE spec's bit numbering (spec uses 1-indexed,
/// so subtract 1 when calling this function).
pub fn extract_bits(data: &[u8], start_bit: usize, num_bits: usize) -> u32 {
    todo!()
}

/// Pack a value into a byte slice at the given bit position.
/// Used for constructing test data and serializing outbound commands.
pub fn pack_bits(data: &mut [u8], start_bit: usize, num_bits: usize, value: u32) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_single_byte_aligned() {
        let data = [0b10110100];
        assert_eq!(extract_bits(&data, 0, 8), 0b10110100);
    }

    #[test]
    fn extract_low_bits() {
        let data = [0b10110100];
        assert_eq!(extract_bits(&data, 0, 4), 0b0100); // lower nibble
    }

    #[test]
    fn extract_high_bits() {
        let data = [0b10110100];
        assert_eq!(extract_bits(&data, 4, 4), 0b1011); // upper nibble
    }

    #[test]
    fn extract_across_byte_boundary() {
        let data = [0xFF, 0x01]; // binary: 11111111 00000001
        // Bits 4-11 (0-indexed): upper 4 of byte[0] + lower 4 of byte[1]
        // = 1111 from byte[0] + 0001 from byte[1] = 0001_1111 = 0x1F
        assert_eq!(extract_bits(&data, 4, 8), 0x1F);
    }

    #[test]
    fn extract_13_bit_temperature_value() {
        // 900 = (25°C + 20) / 0.05 = 0x384 = 0b0_0011_1000_0100
        // Stored LSB-first starting at bit 0:
        // byte[0] = lower 8 bits = 0x84
        // byte[1] = upper 5 bits = 0x03 (in bits 0-4)
        let data = [0x84, 0x03];
        assert_eq!(extract_bits(&data, 0, 13), 900);
    }

    #[test]
    fn pack_and_extract_round_trip() {
        let mut data = [0u8; 4];
        pack_bits(&mut data, 0, 13, 900);
        pack_bits(&mut data, 13, 13, 1200);
        assert_eq!(extract_bits(&data, 0, 13), 900);
        assert_eq!(extract_bits(&data, 13, 13), 1200);
    }

    #[test]
    fn pack_bits_single_byte() {
        let mut data = [0u8; 1];
        pack_bits(&mut data, 0, 2, 0b11);
        assert_eq!(data[0], 0b11);
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cd sbc-service && cargo test protocol`
Expected: FAIL — `todo!()` panics.

**Step 4: Implement bit extraction**

```rust
pub fn extract_bits(data: &[u8], start_bit: usize, num_bits: usize) -> u32 {
    debug_assert!(num_bits <= 32);
    let mut result: u32 = 0;
    for i in 0..num_bits {
        let bit_pos = start_bit + i;
        let byte_idx = bit_pos / 8;
        let bit_idx = bit_pos % 8;
        if byte_idx < data.len() {
            let bit_val = (data[byte_idx] >> bit_idx) & 1;
            result |= (bit_val as u32) << i;
        }
    }
    result
}

pub fn pack_bits(data: &mut [u8], start_bit: usize, num_bits: usize, value: u32) {
    debug_assert!(num_bits <= 32);
    for i in 0..num_bits {
        let bit_pos = start_bit + i;
        let byte_idx = bit_pos / 8;
        let bit_idx = bit_pos % 8;
        if byte_idx < data.len() {
            let bit_val = ((value >> i) & 1) as u8;
            data[byte_idx] = (data[byte_idx] & !(1 << bit_idx)) | (bit_val << bit_idx);
        }
    }
}
```

**Step 5: Run bit extraction tests**

Run: `cd sbc-service && cargo test protocol::bits`
Expected: All pass.

**Step 6: Write temperature parsing tests**

```rust
// sbc-service/src/protocol/temperature.rs
use super::bits::{extract_bits, pack_bits};
use super::types::ProbeTemperatures;

/// Parse 8 temperatures from 13 bytes of raw temperature data.
/// Each temperature is a 13-bit value packed LSB-first.
/// Formula: (raw * 0.05) - 20 °C
pub fn parse_temperatures(data: &[u8]) -> ProbeTemperatures {
    todo!()
}

/// Encode 8 temperatures into 13 bytes. Used for constructing test data.
#[cfg(test)]
fn encode_temperatures(temps: &[f64; 8]) -> [u8; 13] {
    let mut data = [0u8; 13];
    for i in 0..8 {
        let raw = ((temps[i] + 20.0) / 0.05).round() as u32;
        pack_bits(&mut data, i * 13, 13, raw);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_room_temperature() {
        let encoded = encode_temperatures(&[25.0; 8]);
        let parsed = parse_temperatures(&encoded);
        for i in 0..8 {
            assert!(
                (parsed.values[i] - 25.0).abs() < 0.05,
                "T{}: expected ~25.0, got {}",
                i + 1,
                parsed.values[i]
            );
        }
    }

    #[test]
    fn parse_different_temperatures() {
        let input = [
            -20.0, 0.0, 25.0, 100.0, 150.0, 200.0, 300.0, 369.0,
        ];
        let encoded = encode_temperatures(&input);
        let parsed = parse_temperatures(&encoded);
        for i in 0..8 {
            assert!(
                (parsed.values[i] - input[i]).abs() < 0.05,
                "T{}: expected {}, got {}",
                i + 1,
                input[i],
                parsed.values[i]
            );
        }
    }

    #[test]
    fn parse_minimum_temperature() {
        // raw = 0 → (0 * 0.05) - 20 = -20°C
        let encoded = encode_temperatures(&[-20.0; 8]);
        let parsed = parse_temperatures(&encoded);
        for t in &parsed.values {
            assert!((*t - (-20.0)).abs() < 0.05);
        }
    }

    #[test]
    fn parse_maximum_temperature() {
        // raw = 8191 (max 13-bit) → (8191 * 0.05) - 20 = 389.55°C
        // But spec says max is 369°C, so let's test at 369
        let encoded = encode_temperatures(&[369.0; 8]);
        let parsed = parse_temperatures(&encoded);
        for t in &parsed.values {
            assert!((*t - 369.0).abs() < 0.05);
        }
    }
}
```

**Step 7: Run temperature tests to verify they fail**

Run: `cd sbc-service && cargo test protocol::temperature`
Expected: FAIL.

**Step 8: Implement temperature parsing**

```rust
pub fn parse_temperatures(data: &[u8]) -> ProbeTemperatures {
    assert!(data.len() >= 13, "temperature data must be at least 13 bytes");
    let mut values = [0.0f64; 8];
    for i in 0..8 {
        let raw = extract_bits(data, i * 13, 13);
        values[i] = (raw as f64) * 0.05 - 20.0;
    }
    ProbeTemperatures { values }
}
```

**Step 9: Run all tests**

Run: `cd sbc-service && cargo test`
Expected: All pass.

**Step 10: Wire module into main.rs and create mod.rs**

```rust
// sbc-service/src/protocol/mod.rs
pub mod bits;
pub mod temperature;
pub mod types;
```

Add `mod protocol;` to `main.rs`.

**Step 11: Commit**

```bash
git add sbc-service/src/protocol/ sbc-service/src/main.rs
git commit -m "feat: bit extraction utilities and temperature parser"
```

---

## Task 2: Mode/ID, Battery/Virtual Sensors, Overheating

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/mode_id.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add types**

Append to `types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ProbeMode {
    Normal,
    InstantRead,
    Reserved,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ProbeColor {
    Yellow,
    Grey,
    Color2,
    Color3,
    Color4,
    Color5,
    Color6,
    Color7,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModeId {
    pub mode: ProbeMode,
    pub color: ProbeColor,
    pub id: u8, // 0-7, displayed as ID 1-8
}

/// Which physical sensor (T1-T6) the probe considers "core".
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum VirtualCoreSensor {
    T1, T2, T3, T4, T5, T6,
}

/// Which physical sensor (T4-T7) the probe considers "surface".
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum VirtualSurfaceSensor {
    T4, T5, T6, T7,
}

/// Which physical sensor (T5-T8) the probe considers "ambient".
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum VirtualAmbientSensor {
    T5, T6, T7, T8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatteryVirtualSensors {
    pub battery_ok: bool,
    pub virtual_core: VirtualCoreSensor,
    pub virtual_surface: VirtualSurfaceSensor,
    pub virtual_ambient: VirtualAmbientSensor,
}

/// Bitmask of which sensors (T1-T8) are overheating.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverheatingSensors {
    pub sensors: [bool; 8], // Index 0 = T1, index 7 = T8
}
```

**Step 2: Write tests and implement**

```rust
// sbc-service/src/protocol/mode_id.rs
use super::bits::extract_bits;
use super::types::*;

/// Parse the 1-byte Mode/ID field.
/// Bits 1-2: Mode, Bits 3-5: Color, Bits 6-8: Probe ID (spec is 1-indexed).
pub fn parse_mode_id(byte: u8) -> ModeId {
    let data = [byte];
    let mode_raw = extract_bits(&data, 0, 2);
    let color_raw = extract_bits(&data, 2, 3);
    let id_raw = extract_bits(&data, 5, 3);

    ModeId {
        mode: match mode_raw {
            0 => ProbeMode::Normal,
            1 => ProbeMode::InstantRead,
            2 => ProbeMode::Reserved,
            _ => ProbeMode::Error,
        },
        color: match color_raw {
            0 => ProbeColor::Yellow,
            1 => ProbeColor::Grey,
            2 => ProbeColor::Color2,
            3 => ProbeColor::Color3,
            4 => ProbeColor::Color4,
            5 => ProbeColor::Color5,
            6 => ProbeColor::Color6,
            _ => ProbeColor::Color7,
        },
        id: id_raw as u8,
    }
}

/// Parse the 1-byte Battery Status and Virtual Sensors field.
/// Bit 1: battery (0=OK, 1=low). Bits 2-8: virtual sensors (7-bit field).
/// Virtual sensors sub-field: bits 1-3 core (3-bit), 4-5 surface (2-bit), 6-7 ambient (2-bit).
pub fn parse_battery_virtual_sensors(byte: u8) -> BatteryVirtualSensors {
    let data = [byte];
    let battery_low = extract_bits(&data, 0, 1) == 1;
    let core_raw = extract_bits(&data, 1, 3);
    let surface_raw = extract_bits(&data, 4, 2);
    let ambient_raw = extract_bits(&data, 6, 2);

    BatteryVirtualSensors {
        battery_ok: !battery_low,
        virtual_core: match core_raw {
            0 => VirtualCoreSensor::T1,
            1 => VirtualCoreSensor::T2,
            2 => VirtualCoreSensor::T3,
            3 => VirtualCoreSensor::T4,
            4 => VirtualCoreSensor::T5,
            _ => VirtualCoreSensor::T6,
        },
        virtual_surface: match surface_raw {
            0 => VirtualSurfaceSensor::T4,
            1 => VirtualSurfaceSensor::T5,
            2 => VirtualSurfaceSensor::T6,
            _ => VirtualSurfaceSensor::T7,
        },
        virtual_ambient: match ambient_raw {
            0 => VirtualAmbientSensor::T5,
            1 => VirtualAmbientSensor::T6,
            2 => VirtualAmbientSensor::T7,
            _ => VirtualAmbientSensor::T8,
        },
    }
}

/// Parse the 1-byte overheating sensors bitmask.
/// Each bit indicates overheating for a sensor. MSB=T8, LSB=T1.
/// But the spec says bit 1=T8, bit 8=T1 — which in our 0-indexed extraction
/// means bit 0 of the byte = T8 status (overheating sensors use reversed order).
///
/// Actually re-reading the spec: Bit 1 (MSB side) = T8, Bit 8 (LSB side) = T1.
/// The spec table lists them top-to-bottom as T8, T7, ..., T1.
/// In the byte: bit 7 = T8, bit 0 = T1 (standard MSB-to-LSB in spec tables).
///
/// Wait — the spec uses "Bits 1-8" where bit 1 is described first as T8.
/// Given the overall LSB-first convention, bit 1 (0-indexed: bit 0) = T8?
/// No — the overheating sensors spec says "MSB is T8, LSB is T1", which means
/// bit 7 = T8, bit 0 = T1 in standard byte representation.
pub fn parse_overheating_sensors(byte: u8) -> OverheatingSensors {
    let mut sensors = [false; 8];
    for i in 0..8 {
        sensors[i] = (byte >> i) & 1 == 1;
    }
    OverheatingSensors { sensors }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_id_normal_yellow_id1() {
        // Mode=0 (Normal), Color=0 (Yellow), ID=0 → 0b000_000_00
        let result = parse_mode_id(0b000_000_00);
        assert_eq!(result.mode, ProbeMode::Normal);
        assert_eq!(result.color, ProbeColor::Yellow);
        assert_eq!(result.id, 0);
    }

    #[test]
    fn parse_mode_id_instant_read_grey_id3() {
        // Mode=1 (InstantRead), Color=1 (Grey), ID=2 (displayed as 3)
        // Binary: ID(010) Color(001) Mode(01) = 0b010_001_01 = 0x45
        let result = parse_mode_id(0b010_001_01);
        assert_eq!(result.mode, ProbeMode::InstantRead);
        assert_eq!(result.color, ProbeColor::Grey);
        assert_eq!(result.id, 2);
    }

    #[test]
    fn parse_mode_id_error() {
        // Mode=3 (Error)
        let result = parse_mode_id(0b000_000_11);
        assert_eq!(result.mode, ProbeMode::Error);
    }

    #[test]
    fn parse_battery_ok_core_t1() {
        // Battery=0 (OK), Core=0 (T1), Surface=0 (T4), Ambient=0 (T5)
        let result = parse_battery_virtual_sensors(0b00_00_000_0);
        assert!(result.battery_ok);
        assert_eq!(result.virtual_core, VirtualCoreSensor::T1);
        assert_eq!(result.virtual_surface, VirtualSurfaceSensor::T4);
        assert_eq!(result.virtual_ambient, VirtualAmbientSensor::T5);
    }

    #[test]
    fn parse_battery_low() {
        let result = parse_battery_virtual_sensors(0b00_00_000_1);
        assert!(!result.battery_ok);
    }

    #[test]
    fn parse_virtual_sensors_varied() {
        // Battery=0, Core=3 (T4), Surface=2 (T6), Ambient=3 (T8)
        // byte: ambient(11) surface(10) core(011) battery(0) = 0b11_10_011_0 = 0xE6
        let result = parse_battery_virtual_sensors(0b11_10_011_0);
        assert!(result.battery_ok);
        assert_eq!(result.virtual_core, VirtualCoreSensor::T4);
        assert_eq!(result.virtual_surface, VirtualSurfaceSensor::T6);
        assert_eq!(result.virtual_ambient, VirtualAmbientSensor::T8);
    }

    #[test]
    fn parse_overheating_none() {
        let result = parse_overheating_sensors(0x00);
        assert!(result.sensors.iter().all(|&s| !s));
    }

    #[test]
    fn parse_overheating_t1_only() {
        // LSB = T1
        let result = parse_overheating_sensors(0b00000001);
        assert!(result.sensors[0]); // T1
        assert!(!result.sensors[1]); // T2
    }

    #[test]
    fn parse_overheating_all() {
        let result = parse_overheating_sensors(0xFF);
        assert!(result.sensors.iter().all(|&s| s));
    }
}
```

**Step 3: Run tests**

Run: `cd sbc-service && cargo test protocol::mode_id`
Expected: All pass.

**Step 4: Update mod.rs and commit**

```rust
// sbc-service/src/protocol/mod.rs
pub mod bits;
pub mod mode_id;
pub mod temperature;
pub mod types;
```

```bash
git add sbc-service/src/protocol/
git commit -m "feat: mode/ID, battery/virtual sensors, overheating parsers"
```

---

## Task 3: Prediction Status

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/prediction.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add prediction types to types.rs**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PredictionState {
    ProbeNotInserted,
    ProbeInserted,
    Warming,
    Predicting,
    RemovalPredictionDone,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PredictionMode {
    None,
    TimeToRemoval,
    RemovalAndResting,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PredictionType {
    None,
    Removal,
    Resting,
    Reserved,
}

/// Prediction status from Probe Status notifications (7 bytes).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PredictionStatus {
    pub state: PredictionState,
    pub mode: PredictionMode,
    pub prediction_type: PredictionType,
    pub set_point_celsius: f64,
    pub heat_start_celsius: f64,
    pub seconds_remaining: u32,
    pub estimated_core_celsius: f64,
}

/// Prediction log entry from Read Logs responses (7 bytes).
/// Different layout from PredictionStatus — no heat_start field,
/// and virtual sensors are packed in front.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PredictionLog {
    pub virtual_core: VirtualCoreSensor,
    pub virtual_surface: VirtualSurfaceSensor,
    pub virtual_ambient: VirtualAmbientSensor,
    pub state: PredictionState,
    pub mode: PredictionMode,
    pub prediction_type: PredictionType,
    pub set_point_celsius: f64,
    pub seconds_remaining: u32,
    pub estimated_core_celsius: f64,
}
```

**Step 2: Write tests and implement**

```rust
// sbc-service/src/protocol/prediction.rs
use super::bits::{extract_bits, pack_bits};
use super::mode_id; // for virtual sensor parsing in prediction log
use super::types::*;

fn parse_prediction_state(raw: u32) -> PredictionState {
    match raw {
        0 => PredictionState::ProbeNotInserted,
        1 => PredictionState::ProbeInserted,
        2 => PredictionState::Warming,
        3 => PredictionState::Predicting,
        4 => PredictionState::RemovalPredictionDone,
        _ => PredictionState::Unknown,
    }
}

fn parse_prediction_mode(raw: u32) -> PredictionMode {
    match raw {
        0 => PredictionMode::None,
        1 => PredictionMode::TimeToRemoval,
        2 => PredictionMode::RemovalAndResting,
        _ => PredictionMode::Reserved,
    }
}

fn parse_prediction_type(raw: u32) -> PredictionType {
    match raw {
        0 => PredictionType::None,
        1 => PredictionType::Removal,
        2 => PredictionType::Resting,
        _ => PredictionType::Reserved,
    }
}

/// Parse the 7-byte Prediction Status field from Probe Status notifications.
/// Layout (1-indexed bits from spec, converted to 0-indexed):
///   Bits 0-3: state (4 bits)
///   Bits 4-5: mode (2 bits)
///   Bits 6-7: type (2 bits)
///   Bits 8-17: set point temp (10 bits, raw * 0.1 °C)
///   Bits 18-27: heat start temp (10 bits, raw * 0.1 °C)
///   Bits 28-44: seconds remaining (17 bits)
///   Bits 45-55: estimated core (11 bits, raw * 0.1 - 20 °C)
pub fn parse_prediction_status(data: &[u8]) -> PredictionStatus {
    assert!(data.len() >= 7, "prediction status must be at least 7 bytes");

    PredictionStatus {
        state: parse_prediction_state(extract_bits(data, 0, 4)),
        mode: parse_prediction_mode(extract_bits(data, 4, 2)),
        prediction_type: parse_prediction_type(extract_bits(data, 6, 2)),
        set_point_celsius: extract_bits(data, 8, 10) as f64 * 0.1,
        heat_start_celsius: extract_bits(data, 18, 10) as f64 * 0.1,
        seconds_remaining: extract_bits(data, 28, 17),
        estimated_core_celsius: extract_bits(data, 45, 11) as f64 * 0.1 - 20.0,
    }
}

/// Parse the 7-byte Prediction Log from Read Logs responses.
/// Layout (0-indexed):
///   Bits 0-6: virtual sensors (7 bits)
///   Bits 7-10: state (4 bits)
///   Bits 11-12: mode (2 bits)
///   Bits 13-14: type (2 bits)
///   Bits 15-24: set point temp (10 bits)
///   Bits 25-41: seconds remaining (17 bits)
///   Bits 42-52: estimated core (11 bits)
pub fn parse_prediction_log(data: &[u8]) -> PredictionLog {
    assert!(data.len() >= 7, "prediction log must be at least 7 bytes");

    let core_raw = extract_bits(data, 0, 3);
    let surface_raw = extract_bits(data, 3, 2);
    let ambient_raw = extract_bits(data, 5, 2);

    PredictionLog {
        virtual_core: match core_raw {
            0 => VirtualCoreSensor::T1,
            1 => VirtualCoreSensor::T2,
            2 => VirtualCoreSensor::T3,
            3 => VirtualCoreSensor::T4,
            4 => VirtualCoreSensor::T5,
            _ => VirtualCoreSensor::T6,
        },
        virtual_surface: match surface_raw {
            0 => VirtualSurfaceSensor::T4,
            1 => VirtualSurfaceSensor::T5,
            2 => VirtualSurfaceSensor::T6,
            _ => VirtualSurfaceSensor::T7,
        },
        virtual_ambient: match ambient_raw {
            0 => VirtualAmbientSensor::T5,
            1 => VirtualAmbientSensor::T6,
            2 => VirtualAmbientSensor::T7,
            _ => VirtualAmbientSensor::T8,
        },
        state: parse_prediction_state(extract_bits(data, 7, 4)),
        mode: parse_prediction_mode(extract_bits(data, 11, 2)),
        prediction_type: parse_prediction_type(extract_bits(data, 13, 2)),
        set_point_celsius: extract_bits(data, 15, 10) as f64 * 0.1,
        seconds_remaining: extract_bits(data, 25, 17),
        estimated_core_celsius: extract_bits(data, 42, 11) as f64 * 0.1 - 20.0,
    }
}

/// Encode a set-prediction command payload (2 bytes).
/// Bits 0-9: set point (raw = celsius / 0.1), Bits 10-11: mode.
pub fn encode_set_prediction(set_point_celsius: f64, mode: PredictionMode) -> [u8; 2] {
    let mut data = [0u8; 2];
    let raw_temp = (set_point_celsius / 0.1).round() as u32;
    let raw_mode = match mode {
        PredictionMode::None => 0,
        PredictionMode::TimeToRemoval => 1,
        PredictionMode::RemovalAndResting => 2,
        PredictionMode::Reserved => 3,
    };
    pack_bits(&mut data, 0, 10, raw_temp);
    pack_bits(&mut data, 10, 2, raw_mode);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_prediction_status(
        state: u32, mode: u32, ptype: u32,
        set_point_raw: u32, heat_start_raw: u32,
        seconds: u32, est_core_raw: u32,
    ) -> [u8; 7] {
        let mut data = [0u8; 7];
        pack_bits(&mut data, 0, 4, state);
        pack_bits(&mut data, 4, 2, mode);
        pack_bits(&mut data, 6, 2, ptype);
        pack_bits(&mut data, 8, 10, set_point_raw);
        pack_bits(&mut data, 18, 10, heat_start_raw);
        pack_bits(&mut data, 28, 17, seconds);
        pack_bits(&mut data, 45, 11, est_core_raw);
        data
    }

    #[test]
    fn parse_prediction_predicting_state() {
        // 95°C set point = 950 raw, 25°C heat start = 250 raw,
        // 2847 seconds remaining, 72°C estimated core = (72+20)/0.1 = 920 raw
        let data = build_prediction_status(3, 1, 1, 950, 250, 2847, 920);
        let result = parse_prediction_status(&data);

        assert_eq!(result.state, PredictionState::Predicting);
        assert_eq!(result.mode, PredictionMode::TimeToRemoval);
        assert_eq!(result.prediction_type, PredictionType::Removal);
        assert!((result.set_point_celsius - 95.0).abs() < 0.1);
        assert!((result.heat_start_celsius - 25.0).abs() < 0.1);
        assert_eq!(result.seconds_remaining, 2847);
        assert!((result.estimated_core_celsius - 72.0).abs() < 0.1);
    }

    #[test]
    fn parse_prediction_not_inserted() {
        let data = build_prediction_status(0, 0, 0, 0, 0, 0, 0);
        let result = parse_prediction_status(&data);
        assert_eq!(result.state, PredictionState::ProbeNotInserted);
        assert_eq!(result.mode, PredictionMode::None);
    }

    #[test]
    fn parse_prediction_done() {
        let data = build_prediction_status(4, 1, 1, 950, 250, 0, 950);
        let result = parse_prediction_status(&data);
        assert_eq!(result.state, PredictionState::RemovalPredictionDone);
        assert_eq!(result.seconds_remaining, 0);
        assert!((result.estimated_core_celsius - 75.0).abs() < 0.1); // (950*0.1)-20 = 75
    }

    #[test]
    fn encode_set_prediction_round_trip() {
        let encoded = encode_set_prediction(95.0, PredictionMode::TimeToRemoval);
        let set_point = extract_bits(&encoded, 0, 10) as f64 * 0.1;
        let mode = extract_bits(&encoded, 10, 2);
        assert!((set_point - 95.0).abs() < 0.1);
        assert_eq!(mode, 1); // TimeToRemoval
    }
}
```

**Step 3: Run tests, update mod.rs, commit**

Run: `cd sbc-service && cargo test protocol::prediction`

```rust
// Add to protocol/mod.rs
pub mod prediction;
```

```bash
git add sbc-service/src/protocol/
git commit -m "feat: prediction status and prediction log parsers"
```

---

## Task 4: Food Safe Data + Status

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/food_safe.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add types**

Append to `types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FoodSafeMode {
    Simplified,
    Integrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FoodSafeServing {
    ServedImmediately,
    CookedAndChilled,
}

/// Food Safe configuration parameters (10 bytes).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoodSafeData {
    pub mode: FoodSafeMode,
    pub product: u16,
    pub serving: FoodSafeServing,
    pub threshold_celsius: f64,
    pub z_value: f64,
    pub reference_temp_celsius: f64,
    pub d_value: f64,
    pub target_log_reduction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FoodSafeState {
    NotSafe,
    Safe,
    SafetyImpossible,
}

/// Food Safe current status (8 bytes).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoodSafeStatus {
    pub state: FoodSafeState,
    pub log_reduction: f64,
    pub seconds_above_threshold: u16,
    pub log_sequence_number: u32,
}
```

**Step 2: Write tests and implement**

```rust
// sbc-service/src/protocol/food_safe.rs
use super::bits::{extract_bits, pack_bits};
use super::types::*;

/// Parse the 10-byte Food Safe Data field.
/// Layout (0-indexed):
///   Bits 0-2: mode (3 bits)
///   Bits 3-12: product (10 bits)
///   Bits 13-15: serving (3 bits)
///   Bits 16-28: threshold temp (13 bits, raw * 0.05 °C)
///   Bits 29-41: Z-value (13 bits, raw * 0.05)
///   Bits 42-54: reference temp (13 bits, raw * 0.05 °C)
///   Bits 55-67: D-value (13 bits, raw * 0.05)
///   Bits 68-75: target log reduction (8 bits, raw * 0.1)
pub fn parse_food_safe_data(data: &[u8]) -> FoodSafeData {
    assert!(data.len() >= 10, "food safe data must be at least 10 bytes");

    FoodSafeData {
        mode: match extract_bits(data, 0, 3) {
            0 => FoodSafeMode::Simplified,
            _ => FoodSafeMode::Integrated,
        },
        product: extract_bits(data, 3, 10) as u16,
        serving: match extract_bits(data, 13, 3) {
            0 => FoodSafeServing::ServedImmediately,
            _ => FoodSafeServing::CookedAndChilled,
        },
        threshold_celsius: extract_bits(data, 16, 13) as f64 * 0.05,
        z_value: extract_bits(data, 29, 13) as f64 * 0.05,
        reference_temp_celsius: extract_bits(data, 42, 13) as f64 * 0.05,
        d_value: extract_bits(data, 55, 13) as f64 * 0.05,
        target_log_reduction: extract_bits(data, 68, 8) as f64 * 0.1,
    }
}

/// Parse the 8-byte Food Safe Status field.
/// Layout (0-indexed):
///   Bits 0-2: state (3 bits)
///   Bits 3-10: log reduction (8 bits, raw * 0.1)
///   Bits 11-26: seconds above threshold (16 bits)
///   Bits 27-58: food safe log sequence number (32 bits)
pub fn parse_food_safe_status(data: &[u8]) -> FoodSafeStatus {
    assert!(data.len() >= 8, "food safe status must be at least 8 bytes");

    FoodSafeStatus {
        state: match extract_bits(data, 0, 3) {
            0 => FoodSafeState::NotSafe,
            1 => FoodSafeState::Safe,
            2 => FoodSafeState::SafetyImpossible,
            _ => FoodSafeState::NotSafe,
        },
        log_reduction: extract_bits(data, 3, 8) as f64 * 0.1,
        seconds_above_threshold: extract_bits(data, 11, 16) as u16,
        log_sequence_number: extract_bits(data, 27, 32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_food_safe_data(
        mode: u32, product: u32, serving: u32,
        threshold_raw: u32, z_raw: u32, ref_raw: u32, d_raw: u32,
        log_red_raw: u32,
    ) -> [u8; 10] {
        let mut data = [0u8; 10];
        pack_bits(&mut data, 0, 3, mode);
        pack_bits(&mut data, 3, 10, product);
        pack_bits(&mut data, 13, 3, serving);
        pack_bits(&mut data, 16, 13, threshold_raw);
        pack_bits(&mut data, 29, 13, z_raw);
        pack_bits(&mut data, 42, 13, ref_raw);
        pack_bits(&mut data, 55, 13, d_raw);
        pack_bits(&mut data, 68, 8, log_red_raw);
        data
    }

    #[test]
    fn parse_simplified_poultry() {
        // Simplified mode, product=1 (Any poultry), served immediately
        // threshold=165°F ≈ 73.9°C → raw = 73.9/0.05 = 1478
        let data = build_food_safe_data(0, 1, 0, 1478, 0, 0, 0, 0);
        let result = parse_food_safe_data(&data);
        assert_eq!(result.mode, FoodSafeMode::Simplified);
        assert_eq!(result.product, 1);
        assert_eq!(result.serving, FoodSafeServing::ServedImmediately);
        assert!((result.threshold_celsius - 73.9).abs() < 0.05);
    }

    #[test]
    fn parse_integrated_mode() {
        let data = build_food_safe_data(1, 0, 0, 0, 0, 0, 0, 70);
        let result = parse_food_safe_data(&data);
        assert_eq!(result.mode, FoodSafeMode::Integrated);
        assert!((result.target_log_reduction - 7.0).abs() < 0.1);
    }

    fn build_food_safe_status(
        state: u32, log_red_raw: u32, seconds: u32, seq: u32,
    ) -> [u8; 8] {
        let mut data = [0u8; 8];
        pack_bits(&mut data, 0, 3, state);
        pack_bits(&mut data, 3, 8, log_red_raw);
        pack_bits(&mut data, 11, 16, seconds);
        pack_bits(&mut data, 27, 32, seq);
        data
    }

    #[test]
    fn parse_food_safe_status_not_safe() {
        let data = build_food_safe_status(0, 0, 120, 500);
        let result = parse_food_safe_status(&data);
        assert_eq!(result.state, FoodSafeState::NotSafe);
        assert_eq!(result.seconds_above_threshold, 120);
        assert_eq!(result.log_sequence_number, 500);
    }

    #[test]
    fn parse_food_safe_status_safe() {
        let data = build_food_safe_status(1, 70, 3600, 1200);
        let result = parse_food_safe_status(&data);
        assert_eq!(result.state, FoodSafeState::Safe);
        assert!((result.log_reduction - 7.0).abs() < 0.1);
        assert_eq!(result.seconds_above_threshold, 3600);
    }
}
```

**Step 3: Run tests, update mod.rs, commit**

```rust
// Add to protocol/mod.rs
pub mod food_safe;
```

```bash
git add sbc-service/src/protocol/
git commit -m "feat: food safe data and status parsers"
```

---

## Task 5: Alarm Status

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/alarm.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add types**

```rust
// Append to types.rs

/// Status of a single alarm channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlarmStatus {
    pub set: bool,
    pub tripped: bool,
    pub alarming: bool,
    pub temperature_celsius: f64,
}

/// All 11 alarm channels (T1-T8, Core, Surface, Ambient) for high and low.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeAlarms {
    pub high: [AlarmStatus; 11],
    pub low: [AlarmStatus; 11],
}
```

**Step 2: Write tests and implement**

```rust
// sbc-service/src/protocol/alarm.rs
use super::bits::{extract_bits, pack_bits};
use super::types::*;

/// Parse a single 16-bit (2-byte) alarm status.
/// Bit 0: set, Bit 1: tripped, Bit 2: alarming, Bits 3-15: temperature (13 bits).
/// Temperature formula: (raw * 0.1) - 20 °C
pub fn parse_alarm_status(data: &[u8]) -> AlarmStatus {
    assert!(data.len() >= 2);
    AlarmStatus {
        set: extract_bits(data, 0, 1) == 1,
        tripped: extract_bits(data, 1, 1) == 1,
        alarming: extract_bits(data, 2, 1) == 1,
        temperature_celsius: extract_bits(data, 3, 13) as f64 * 0.1 - 20.0,
    }
}

/// Parse the full alarm arrays: 22 bytes high + 22 bytes low = 44 bytes total.
/// Each array is 11 alarms × 2 bytes: T1, T2, T3, T4, T5, T6, T7, T8, Core, Surface, Ambient.
pub fn parse_probe_alarms(data: &[u8]) -> ProbeAlarms {
    assert!(data.len() >= 44, "alarm data must be at least 44 bytes");

    let mut high = std::array::from_fn(|_| AlarmStatus {
        set: false, tripped: false, alarming: false, temperature_celsius: 0.0,
    });
    let mut low = high.clone();

    for i in 0..11 {
        high[i] = parse_alarm_status(&data[i * 2..]);
        low[i] = parse_alarm_status(&data[22 + i * 2..]);
    }

    ProbeAlarms { high, low }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_alarm(set: bool, tripped: bool, alarming: bool, temp_celsius: f64) -> [u8; 2] {
        let mut data = [0u8; 2];
        pack_bits(&mut data, 0, 1, set as u32);
        pack_bits(&mut data, 1, 1, tripped as u32);
        pack_bits(&mut data, 2, 1, alarming as u32);
        let raw_temp = ((temp_celsius + 20.0) / 0.1).round() as u32;
        pack_bits(&mut data, 3, 13, raw_temp);
        data
    }

    #[test]
    fn parse_alarm_not_set() {
        let data = build_alarm(false, false, false, 0.0);
        let result = parse_alarm_status(&data);
        assert!(!result.set);
        assert!(!result.tripped);
        assert!(!result.alarming);
    }

    #[test]
    fn parse_alarm_set_and_tripped() {
        let data = build_alarm(true, true, true, 95.0);
        let result = parse_alarm_status(&data);
        assert!(result.set);
        assert!(result.tripped);
        assert!(result.alarming);
        assert!((result.temperature_celsius - 95.0).abs() < 0.1);
    }

    #[test]
    fn parse_full_alarm_array() {
        let mut data = [0u8; 44];
        // Set high alarm for T1 (index 0) at 95°C
        let alarm = build_alarm(true, false, false, 95.0);
        data[0..2].copy_from_slice(&alarm);
        // Set low alarm for Core (index 8 in low array, offset 22+16)
        let alarm = build_alarm(true, true, true, 50.0);
        data[38..40].copy_from_slice(&alarm);

        let result = parse_probe_alarms(&data);
        assert!(result.high[0].set);
        assert!((result.high[0].temperature_celsius - 95.0).abs() < 0.1);
        assert!(result.low[8].set);
        assert!(result.low[8].alarming);
    }
}
```

**Step 3: Run tests, update mod.rs, commit**

```bash
git add sbc-service/src/protocol/
git commit -m "feat: alarm status parser"
```

---

## Task 6: Advertisement Identity And Probe Advertising Parsers

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/advertising.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add advertisement types**

Identity parsing in this phase is family-specific. The parser should not infer canonical serial format from product type alone.

```rust
// Append to types.rs

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AdvertisementFamily {
    DirectProbe,
    NodeRepeatedProbe,
    NodeSelf,
}

/// Canonical identity extracted from an advertisement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdvertisementIdentity {
    pub family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
}

/// Parsed probe-format advertising payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeAdvertisingData {
    pub probe_serial_number: u32,
    pub temperatures: ProbeTemperatures,
    pub mode_id: ModeId,
    pub battery_virtual: BatteryVirtualSensors,
    pub network_info: u8,
    pub overheating: OverheatingSensors,
}
```

**Step 2: Write tests and implement**

```rust
// sbc-service/src/protocol/advertising.rs
use super::mode_id::{parse_battery_virtual_sensors, parse_mode_id, parse_overheating_sensors};
use super::temperature::parse_temperatures;
use super::types::*;

/// Parse canonical identity from a Combustion advertisement payload.
///
/// `data` is the manufacturer data after the 2-byte company ID.
pub fn parse_advertisement_identity(
    family: AdvertisementFamily,
    data: &[u8],
) -> AdvertisementIdentity {
    assert!(!data.is_empty(), "advertisement data must not be empty");

    let product_type = ProductType::from_byte(data[0]);
    let serial_number = match family {
        AdvertisementFamily::DirectProbe | AdvertisementFamily::NodeRepeatedProbe => {
            assert!(data.len() >= 5, "probe advertisement must include 4-byte serial");
            let serial = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
            format!("{serial:08X}")
        }
        AdvertisementFamily::NodeSelf => {
            assert!(
                data.len() >= 11,
                "node self-advertisement must include 10-byte serial"
            );
            std::str::from_utf8(&data[1..11])
                .expect("node serial must be valid UTF-8")
                .trim_end_matches('\0')
                .to_string()
        }
    };

    AdvertisementIdentity {
        family,
        product_type,
        serial_number,
    }
}

/// Parse the probe-format manufacturer specific data used by direct probe
/// advertisements and node repeated-probe advertisements.
///
/// Layout:
///   Bytes 0-3: probe serial number (u32 LE)
///   Bytes 4-16: raw temperature data (13 bytes)
///   Byte 17: mode/ID
///   Byte 18: battery status and virtual sensors
///   Byte 19: network information
///   Byte 20: overheating sensors
///
/// Note: The input is the manufacturer data AFTER the product type byte,
/// so byte 0 here = byte 1 of the full manufacturer data (byte 0 was product type).
pub fn parse_probe_advertising_data(data: &[u8]) -> ProbeAdvertisingData {
    assert!(
        data.len() >= 21,
        "probe advertising data must be at least 21 bytes (after product type)"
    );

    let probe_serial_number = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let temperatures = parse_temperatures(&data[4..17]);
    let mode_id = parse_mode_id(data[17]);
    let battery_virtual = parse_battery_virtual_sensors(data[18]);
    let network_info = data[19];
    let overheating = parse_overheating_sensors(data[20]);

    ProbeAdvertisingData {
        probe_serial_number,
        temperatures,
        mode_id,
        battery_virtual,
        network_info,
        overheating,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::bits::pack_bits;

    fn build_probe_advertising_data(serial: u32, temp_celsius: f64, mode: u8) -> Vec<u8> {
        let mut data = vec![0u8; 21];
        // Probe serial number (LE)
        data[0..4].copy_from_slice(&serial.to_le_bytes());
        // Temperatures (all same value)
        for i in 0..8 {
            let raw = ((temp_celsius + 20.0) / 0.05).round() as u32;
            pack_bits(&mut data[4..17], i * 13, 13, raw);
        }
        // Mode/ID
        data[17] = mode;
        // Battery/virtual (defaults)
        data[18] = 0x00;
        // Network info
        data[19] = 0x00;
        // Overheating
        data[20] = 0x00;
        data
    }

    #[test]
    fn parse_probe_advertising_serial_number() {
        let data = build_probe_advertising_data(0x10005205, 25.0, 0x00);
        let result = parse_probe_advertising_data(&data);
        assert_eq!(result.probe_serial_number, 0x10005205);
    }

    #[test]
    fn parse_probe_advertising_temperatures() {
        let data = build_probe_advertising_data(0x01, 72.5, 0x00);
        let result = parse_probe_advertising_data(&data);
        for t in &result.temperatures.values {
            assert!((*t - 72.5).abs() < 0.05);
        }
    }

    #[test]
    fn parse_probe_advertising_instant_read() {
        // Mode=1 (InstantRead)
        let data = build_probe_advertising_data(0x01, 25.0, 0b000_000_01);
        let result = parse_probe_advertising_data(&data);
        assert_eq!(result.mode_id.mode, ProbeMode::InstantRead);
    }

    #[test]
    fn parse_probe_advertisement_identity() {
        let identity = parse_advertisement_identity(
            AdvertisementFamily::DirectProbe,
            &[0x01, 0xDD, 0xCC, 0xBB, 0xAA],
        );
        assert_eq!(identity.family, AdvertisementFamily::DirectProbe);
        assert_eq!(identity.product_type, ProductType::PredictiveProbe);
        assert_eq!(identity.serial_number, "AABBCCDD");
    }

    #[test]
    fn parse_node_advertisement_identity() {
        let identity = parse_advertisement_identity(
            AdvertisementFamily::NodeSelf,
            b"\x05CR100010EB",
        );
        assert_eq!(identity.family, AdvertisementFamily::NodeSelf);
        assert_eq!(identity.product_type, ProductType::Booster);
        assert_eq!(identity.serial_number, "CR100010EB");
    }
}
```

**Step 3: Run tests, update mod.rs, commit**

```bash
git add sbc-service/src/protocol/
git commit -m "feat: add advertisement identity and probe advertising parsers"
```

---

## Task 7: CRC-16-CCITT + UART Frame Parser

**Files:**

- Create: `sbc-service/src/protocol/crc.rs`
- Create: `sbc-service/src/protocol/uart.rs`
- Modify: `sbc-service/src/protocol/types.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add UART types**

```rust
// Append to types.rs

/// A parsed UART frame from a MeatNet node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum UartFrame {
    /// An unsolicited request message (e.g., ProbeStatus, Heartbeat).
    Request {
        message_type: u8,
        request_id: u32,
        payload: Vec<u8>,
    },
    /// A response to a command we sent.
    Response {
        message_type: u8,
        request_id: u32,
        response_id: u32,
        success: bool,
        payload: Vec<u8>,
    },
}

impl UartFrame {
    pub fn message_type(&self) -> u8 {
        match self {
            UartFrame::Request { message_type, .. } => *message_type,
            UartFrame::Response { message_type, .. } => *message_type,
        }
    }

    /// The base message type with the response bit stripped.
    pub fn base_message_type(&self) -> u8 {
        self.message_type() & 0x7F
    }

    pub fn payload(&self) -> &[u8] {
        match self {
            UartFrame::Request { payload, .. } => payload,
            UartFrame::Response { payload, .. } => payload,
        }
    }
}
```

**Step 2: Write CRC tests and implement**

```rust
// sbc-service/src/protocol/crc.rs

/// Compute CRC-16-CCITT-FALSE over the given data.
/// Polynomial: 0x1021, Initial value: 0xFFFF.
/// Used to validate inbound UART messages and compute CRC for outbound.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_empty() {
        assert_eq!(crc16_ccitt(&[]), 0xFFFF);
    }

    #[test]
    fn crc_known_value() {
        // "123456789" → CRC-16/CCITT-FALSE = 0x29B1
        let data = b"123456789";
        assert_eq!(crc16_ccitt(data), 0x29B1);
    }

    #[test]
    fn crc_single_byte() {
        // Deterministic — just verify it doesn't panic
        let result = crc16_ccitt(&[0x45]);
        assert_ne!(result, 0xFFFF); // Should change from initial
    }
}
```

**Step 3: Write UART frame parser**

```rust
// sbc-service/src/protocol/uart.rs
use anyhow::{anyhow, Result};

use super::crc::crc16_ccitt;
use super::types::UartFrame;

const SYNC_BYTE_0: u8 = 0xCA;
const SYNC_BYTE_1: u8 = 0xFE;

/// Parse a single UART frame from a byte slice.
/// The slice should start at the sync bytes (0xCA 0xFE).
///
/// Node request header (10 bytes): sync(2) + CRC(2) + type(1) + request_id(4) + payload_len(1)
/// Node response header (15 bytes): sync(2) + CRC(2) + type(1) + request_id(4) + response_id(4) + success(1) + payload_len(1)
///
/// Response messages have the high bit of message_type set.
pub fn parse_uart_frame(data: &[u8]) -> Result<UartFrame> {
    if data.len() < 10 {
        return Err(anyhow!("frame too short: {} bytes", data.len()));
    }

    if data[0] != SYNC_BYTE_0 || data[1] != SYNC_BYTE_1 {
        return Err(anyhow!(
            "invalid sync bytes: {:02X} {:02X}",
            data[0],
            data[1]
        ));
    }

    let stored_crc = u16::from_le_bytes([data[2], data[3]]);
    let message_type = data[4];
    let is_response = message_type & 0x80 != 0;

    if is_response {
        // Response header: 15 bytes minimum
        if data.len() < 15 {
            return Err(anyhow!("response frame too short: {} bytes", data.len()));
        }

        let request_id = u32::from_le_bytes([data[5], data[6], data[7], data[8]]);
        let response_id = u32::from_le_bytes([data[9], data[10], data[11], data[12]]);
        let success = data[13] == 1;
        let payload_len = data[14] as usize;

        if data.len() < 15 + payload_len {
            return Err(anyhow!(
                "response frame truncated: have {}, need {}",
                data.len(),
                15 + payload_len
            ));
        }

        let payload = data[15..15 + payload_len].to_vec();

        // CRC covers everything after the CRC field: type + request_id + response_id + success + payload_len + payload
        let crc_data = &data[4..15 + payload_len];
        let computed_crc = crc16_ccitt(crc_data);
        if stored_crc != computed_crc {
            return Err(anyhow!(
                "CRC mismatch: stored={:04X}, computed={:04X}",
                stored_crc,
                computed_crc
            ));
        }

        Ok(UartFrame::Response {
            message_type,
            request_id,
            response_id,
            success,
            payload,
        })
    } else {
        // Request header: 10 bytes minimum
        let request_id = u32::from_le_bytes([data[5], data[6], data[7], data[8]]);
        let payload_len = data[9] as usize;

        if data.len() < 10 + payload_len {
            return Err(anyhow!(
                "request frame truncated: have {}, need {}",
                data.len(),
                10 + payload_len
            ));
        }

        let payload = data[10..10 + payload_len].to_vec();

        // CRC covers: type + request_id + payload_len + payload
        let crc_data = &data[4..10 + payload_len];
        let computed_crc = crc16_ccitt(crc_data);
        if stored_crc != computed_crc {
            return Err(anyhow!(
                "CRC mismatch: stored={:04X}, computed={:04X}",
                stored_crc,
                computed_crc
            ));
        }

        Ok(UartFrame::Request {
            message_type,
            request_id,
            payload,
        })
    }
}

/// Serialize an outbound UART request message for a node.
/// Returns the complete frame including sync bytes and CRC.
pub fn build_uart_request(message_type: u8, request_id: u32, payload: &[u8]) -> Vec<u8> {
    let payload_len = payload.len() as u8;

    // Build the CRC-covered portion: type + request_id + payload_len + payload
    let mut crc_data = Vec::with_capacity(6 + payload.len());
    crc_data.push(message_type);
    crc_data.extend_from_slice(&request_id.to_le_bytes());
    crc_data.push(payload_len);
    crc_data.extend_from_slice(payload);

    let crc = crc16_ccitt(&crc_data);

    // Build complete frame: sync + CRC + crc_data
    let mut frame = Vec::with_capacity(4 + crc_data.len());
    frame.push(SYNC_BYTE_0);
    frame.push(SYNC_BYTE_1);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame.extend_from_slice(&crc_data);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_request_round_trip() {
        let frame = build_uart_request(0x05, 0x12345678, &[0xAA, 0xBB]);
        let parsed = parse_uart_frame(&frame).unwrap();
        match parsed {
            UartFrame::Request {
                message_type,
                request_id,
                payload,
            } => {
                assert_eq!(message_type, 0x05);
                assert_eq!(request_id, 0x12345678);
                assert_eq!(payload, vec![0xAA, 0xBB]);
            }
            _ => panic!("expected Request"),
        }
    }

    #[test]
    fn build_and_parse_empty_payload() {
        let frame = build_uart_request(0x08, 0x00000001, &[]);
        let parsed = parse_uart_frame(&frame).unwrap();
        assert_eq!(parsed.base_message_type(), 0x08);
        assert!(parsed.payload().is_empty());
    }

    #[test]
    fn parse_invalid_sync_bytes() {
        let data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(parse_uart_frame(&data).is_err());
    }

    #[test]
    fn parse_bad_crc() {
        let mut frame = build_uart_request(0x05, 0x01, &[0xAA]);
        frame[2] = 0x00; // Corrupt CRC
        frame[3] = 0x00;
        assert!(parse_uart_frame(&frame).is_err());
    }

    #[test]
    fn parse_truncated_frame() {
        let data = [0xCA, 0xFE, 0x00, 0x00, 0x45];
        assert!(parse_uart_frame(&data).is_err());
    }

    #[test]
    fn parse_response_frame() {
        // Build a response manually: sync + CRC + type(0xC5) + req_id + resp_id + success + payload_len + payload
        let message_type: u8 = 0xC5; // 0x45 | 0x80
        let request_id: u32 = 0x11111111;
        let response_id: u32 = 0x22222222;
        let success: u8 = 1;
        let payload: &[u8] = &[0xDD];
        let payload_len: u8 = payload.len() as u8;

        // CRC data
        let mut crc_data = Vec::new();
        crc_data.push(message_type);
        crc_data.extend_from_slice(&request_id.to_le_bytes());
        crc_data.extend_from_slice(&response_id.to_le_bytes());
        crc_data.push(success);
        crc_data.push(payload_len);
        crc_data.extend_from_slice(payload);
        let crc = crc16_ccitt(&crc_data);

        let mut frame = Vec::new();
        frame.push(0xCA);
        frame.push(0xFE);
        frame.extend_from_slice(&crc.to_le_bytes());
        frame.extend_from_slice(&crc_data);

        let parsed = parse_uart_frame(&frame).unwrap();
        match parsed {
            UartFrame::Response {
                message_type: mt,
                request_id: rid,
                response_id: resp_id,
                success: s,
                payload: p,
            } => {
                assert_eq!(mt, 0xC5);
                assert_eq!(rid, 0x11111111);
                assert_eq!(resp_id, 0x22222222);
                assert!(s);
                assert_eq!(p, vec![0xDD]);
            }
            _ => panic!("expected Response"),
        }
    }
}
```

**Step 4: Run tests, update mod.rs, commit**

```bash
git add sbc-service/src/protocol/
git commit -m "feat: CRC-16-CCITT and UART frame parser with serializer"
```

---

## Task 8: Probe Status (0x45) Parser

**Files:**

- Modify: `sbc-service/src/protocol/types.rs`
- Create: `sbc-service/src/protocol/probe_status.rs`
- Modify: `sbc-service/src/protocol/mod.rs`

**Step 1: Add ProbeStatus type**

```rust
// Append to types.rs

/// Fully parsed Probe Status from a node UART 0x45 message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeStatusData {
    pub probe_serial_number: u32,
    pub log_min_sequence: u32,
    pub log_max_sequence: u32,
    pub temperatures: ProbeTemperatures,
    pub mode_id: ModeId,
    pub battery_virtual: BatteryVirtualSensors,
    pub prediction: PredictionStatus,
    pub food_safe_data: FoodSafeData,
    pub food_safe_status: FoodSafeStatus,
    pub network_info: u8,
    pub overheating: OverheatingSensors,
    pub thermometer_preferences: u8,
    pub alarms: ProbeAlarms,
}

/// A parsed Read Logs (0x04) response entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub probe_serial_number: u32,
    pub sequence_number: u32,
    pub temperatures: ProbeTemperatures,
    pub prediction_log: PredictionLog,
}
```

**Step 2: Write parser and tests**

```rust
// sbc-service/src/protocol/probe_status.rs
use super::alarm::parse_probe_alarms;
use super::food_safe::{parse_food_safe_data, parse_food_safe_status};
use super::mode_id::{parse_battery_virtual_sensors, parse_mode_id, parse_overheating_sensors};
use super::prediction::{parse_prediction_log, parse_prediction_status};
use super::temperature::parse_temperatures;
use super::types::*;

/// Parse a Probe Status (0x45) message payload from a node UART notification.
///
/// Payload layout (from meatnet_node_ble_specification.rst):
///   Bytes 0-3: probe serial number (u32 LE)
///   Bytes 4-11: log range (min u32 LE + max u32 LE)
///   Bytes 12-24: raw temperature data (13 bytes)
///   Byte 25: mode/ID
///   Byte 26: battery status and virtual sensors
///   Bytes 27-33: prediction status (7 bytes)
///   Bytes 34-43: food safe data (10 bytes)
///   Bytes 44-51: food safe status (8 bytes)
///   Byte 52: network information
///   Byte 53: overheating sensors
///   Byte 54: thermometer preferences
///   Bytes 55-76: high alarm status array (22 bytes)
///   Bytes 77-98: low alarm status array (22 bytes)
///   Total: 99 bytes
pub fn parse_probe_status(payload: &[u8]) -> ProbeStatusData {
    assert!(
        payload.len() >= 99,
        "probe status payload must be at least 99 bytes, got {}",
        payload.len()
    );

    let probe_serial_number =
        u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let log_min = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    let log_max = u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]);

    ProbeStatusData {
        probe_serial_number,
        log_min_sequence: log_min,
        log_max_sequence: log_max,
        temperatures: parse_temperatures(&payload[12..25]),
        mode_id: parse_mode_id(payload[25]),
        battery_virtual: parse_battery_virtual_sensors(payload[26]),
        prediction: parse_prediction_status(&payload[27..34]),
        food_safe_data: parse_food_safe_data(&payload[34..44]),
        food_safe_status: parse_food_safe_status(&payload[44..52]),
        network_info: payload[52],
        overheating: parse_overheating_sensors(payload[53]),
        thermometer_preferences: payload[54],
        alarms: parse_probe_alarms(&payload[55..99]),
    }
}

/// Parse a Read Logs (0x04) response payload from a node.
///
/// Payload layout:
///   Bytes 0-3: probe serial number (u32 LE)
///   Bytes 4-7: sequence number (u32 LE)
///   Bytes 8-20: raw temperature data (13 bytes)
///   Bytes 21-27: prediction log / virtual sensors and state (7 bytes)
///   Total: 28 bytes
pub fn parse_log_entry(payload: &[u8]) -> LogEntry {
    assert!(
        payload.len() >= 28,
        "log entry payload must be at least 28 bytes, got {}",
        payload.len()
    );

    LogEntry {
        probe_serial_number: u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]),
        sequence_number: u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]),
        temperatures: parse_temperatures(&payload[8..21]),
        prediction_log: parse_prediction_log(&payload[21..28]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::bits::pack_bits;

    fn build_probe_status_payload(serial: u32, log_min: u32, log_max: u32) -> Vec<u8> {
        let mut payload = vec![0u8; 99];
        payload[0..4].copy_from_slice(&serial.to_le_bytes());
        payload[4..8].copy_from_slice(&log_min.to_le_bytes());
        payload[8..12].copy_from_slice(&log_max.to_le_bytes());
        // Leave temperatures, predictions, etc. as zeros (default values)
        payload
    }

    #[test]
    fn parse_probe_status_serial_and_log_range() {
        let payload = build_probe_status_payload(0xAABBCCDD, 100, 500);
        let result = parse_probe_status(&payload);
        assert_eq!(result.probe_serial_number, 0xAABBCCDD);
        assert_eq!(result.log_min_sequence, 100);
        assert_eq!(result.log_max_sequence, 500);
    }

    #[test]
    fn parse_probe_status_default_values() {
        let payload = build_probe_status_payload(0x01, 0, 0);
        let result = parse_probe_status(&payload);
        // All zeros → default parsed values
        assert_eq!(result.mode_id.mode, ProbeMode::Normal);
        assert_eq!(result.prediction.state, PredictionState::ProbeNotInserted);
        assert_eq!(result.food_safe_status.state, FoodSafeState::NotSafe);
    }

    #[test]
    fn parse_log_entry_basic() {
        let mut payload = vec![0u8; 28];
        payload[0..4].copy_from_slice(&0xAABBCCDD_u32.to_le_bytes());
        payload[4..8].copy_from_slice(&42_u32.to_le_bytes());
        // Temperatures all zero → -20°C each

        let result = parse_log_entry(&payload);
        assert_eq!(result.probe_serial_number, 0xAABBCCDD);
        assert_eq!(result.sequence_number, 42);
        assert!((result.temperatures.values[0] - (-20.0)).abs() < 0.05);
    }
}
```

**Step 3: Run tests, update mod.rs, commit**

```bash
git add sbc-service/src/protocol/
git commit -m "feat: Probe Status (0x45) and Read Logs (0x04) parsers"
```

---

## Task 9: Integration — Wire Parsers into Debug Server

**Files:**

- Modify: `sbc-service/src/ble/events.rs`
- Modify: `sbc-service/src/ble/connection.rs`
- Modify: `sbc-service/static/debug.html`

This task enhances the debug server to show parsed results alongside raw bytes. The UART notification handler attempts to parse each frame and includes the parsed data in the event.

**Step 1: Add parsed data to BleEvent**

Add a new field to the `UartNotification` variant in `events.rs`:

```rust
    #[serde(rename = "uart_tx")]
    UartNotification {
        timestamp_ms: u64,
        raw_bytes_hex: String,
        message_type: Option<String>,
        message_type_name: Option<String>,
        byte_count: usize,
        /// JSON-serialized parsed data, if parsing succeeded.
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed: Option<serde_json::Value>,
    },
```

**Step 2: Update uart_notification constructor to attempt parsing**

In `events.rs`, update the `BleEvent::uart_notification` method to try parsing the UART frame and, on success for known message types like 0x45 (ProbeStatus), include the parsed data:

```rust
    pub fn uart_notification(raw_bytes: &[u8]) -> Self {
        let (msg_type, msg_name) = if raw_bytes.len() >= 5
            && raw_bytes[0] == 0xCA
            && raw_bytes[1] == 0xFE
        {
            let mt = raw_bytes[4];
            (
                Some(format!("0x{:02X}", mt)),
                Some(uart_message_type_name(mt).to_string()),
            )
        } else {
            (None, None)
        };

        // Attempt to parse known message types
        let parsed = crate::protocol::uart::parse_uart_frame(raw_bytes)
            .ok()
            .and_then(|frame| {
                match frame.base_message_type() {
                    0x45 => {
                        // ProbeStatus
                        if frame.payload().len() >= 99 {
                            let status = crate::protocol::probe_status::parse_probe_status(
                                frame.payload(),
                            );
                            serde_json::to_value(&status).ok()
                        } else {
                            None
                        }
                    }
                    _ => None, // Other message types added in future phases
                }
            });

        BleEvent::UartNotification {
            timestamp_ms: now_ms(),
            raw_bytes_hex: bytes_to_hex(raw_bytes),
            message_type: msg_type,
            message_type_name: msg_name,
            byte_count: raw_bytes.len(),
            parsed,
        }
    }
```

**Step 3: Update advertising event to include parsed data**

Similarly, add a `parsed` field to `BleEvent::Advertising` and populate it:

```rust
    #[serde(rename = "advertising")]
    Advertising {
        timestamp_ms: u64,
        peripheral_handle: String,
        product_type: String,
        serial_number: String,
        rssi: Option<i16>,
        raw_bytes_hex: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed: Option<serde_json::Value>,
    },
```

Update the `BleEvent::advertising` constructor to parse advertising data.

**Step 4: Update debug UI to show parsed data**

Add a collapsible "Parsed" column to the debug HTML. When a row has `parsed` data, show it as a formatted JSON block that expands on click:

Add to the table header:

```html
<th>Parsed</th>
```

In the JavaScript `appendRow` function, add the parsed column:

```javascript
const parsedStr = event.parsed
  ? "<details><summary>Parsed</summary><pre>" +
    JSON.stringify(event.parsed, null, 2) +
    "</pre></details>"
  : "";
```

And add `'<td>' + parsedStr + '</td>'` to the row HTML.

**Step 5: Run tests and build**

Run: `cd sbc-service && cargo test && cargo build`
Expected: All tests pass, builds successfully.

**Step 6: Test on hardware**

Run: `RUST_LOG=info cargo run`

Open `http://127.0.0.1:3001/debug` on the SBC. ProbeStatus rows should now show a clickable "Parsed" element that expands to reveal the full decoded JSON (temperatures, prediction state, food safety, etc.).
If LAN debug mode is explicitly enabled, use `http://<pi-ip>:3001/debug`.

**Step 7: Commit**

```bash
git add sbc-service/src/ble/events.rs sbc-service/src/ble/connection.rs sbc-service/static/debug.html
git commit -m "feat: parsed protocol data in debug server (side-by-side raw + decoded)"
```

---

## Verification Checklist (End of Phase 3)

1. **All unit tests pass:**

   ```bash
   cd sbc-service && cargo test
   ```

   Expected: All tests pass — bit extraction, temperature, mode/ID, battery/virtual, overheating, prediction, food safe, alarms, CRC, UART frames, probe status, log entries.

2. **Test count sanity check:**

   ```bash
   cd sbc-service && cargo test 2>&1 | grep "test result"
   ```

   Expected: 40+ tests total.

3. **Debug server shows parsed data (hardware test):**

   ```bash
   RUST_LOG=info cargo run
   ```

   Open `http://127.0.0.1:3001/debug` on the SBC:
   if LAN debug mode is explicitly enabled, use `http://<pi-ip>:3001/debug`.
   - [ ] ProbeStatus rows show "Parsed" toggle
   - [ ] Parsed data includes correct temperatures (compare with Combustion app)
   - [ ] Prediction state matches what the Combustion app shows
   - [ ] Serial numbers match real devices

4. **No parsing panics in the wild:**
   Let the service run for 5+ minutes connected to real devices. Check logs for any panics or parsing errors:
   ```bash
   RUST_LOG=info cargo run 2>&1 | grep -i "panic\|error\|assert"
   ```
   Expected: No assertion failures or panics.
