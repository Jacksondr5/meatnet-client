use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;

use super::constants::{
    COMBUSTION_COMPANY_ID, FIRMWARE_REVISION_UUID, HARDWARE_REVISION_UUID, MANUFACTURER_NAME_UUID,
    PROBE_STATUS_CHAR_UUID, PROBE_STATUS_SERVICE_UUID, SERIAL_NUMBER_UUID, UART_RX_UUID,
    UART_SERVICE_UUID, UART_TX_UUID,
};
use super::transport::{
    AdvertisementFamily, BleTransport, ConnectedPeripheral, DeviceInfo, DiscoveryEvent,
    NotificationEvent, NotificationSource, ServiceSummary, WriteMode,
};
use crate::types::ProductType;

pub struct BtleplugTransport {
    adapter: Adapter,
}

impl BtleplugTransport {
    pub async fn new_default() -> Result<Self> {
        let manager = Manager::new()
            .await
            .context("failed to create btleplug manager")?;
        let adapters = manager
            .adapters()
            .await
            .context("failed to enumerate adapters")?;
        let adapter = adapters
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no Bluetooth adapters found"))?;
        Ok(Self { adapter })
    }

    async fn get_peripheral(&self, peripheral_handle: &str) -> Result<Peripheral> {
        let peripherals = self
            .adapter
            .peripherals()
            .await
            .context("failed to enumerate peripherals")?;

        peripherals
            .into_iter()
            .find(|peripheral| format!("{:?}", peripheral.id()) == peripheral_handle)
            .ok_or_else(|| anyhow!("no peripheral matches handle {peripheral_handle}"))
    }
}

#[async_trait]
impl BleTransport for BtleplugTransport {
    async fn scan(&self, duration: Duration) -> Result<Vec<DiscoveryEvent>> {
        self.adapter
            .start_scan(ScanFilter::default())
            .await
            .context("failed to start BLE scan")?;
        tokio::time::sleep(duration).await;

        let peripherals = self
            .adapter
            .peripherals()
            .await
            .context("failed to read discovered peripherals")?;

        let mut events = Vec::new();
        for peripheral in peripherals {
            if let Some(event) = snapshot_peripheral(peripheral).await? {
                events.push(event);
            }
        }
        Ok(events)
    }

    async fn connect(&self, peripheral_handle: &str) -> Result<Box<dyn ConnectedPeripheral>> {
        let peripheral = self.get_peripheral(peripheral_handle).await?;
        if !peripheral.is_connected().await.unwrap_or(false) {
            peripheral
                .connect()
                .await
                .with_context(|| format!("failed to connect to handle {peripheral_handle}"))?;
        }
        peripheral
            .discover_services()
            .await
            .context("failed to discover services")?;

        Ok(Box::new(BtleplugPeripheral { peripheral }))
    }
}

struct BtleplugPeripheral {
    peripheral: Peripheral,
}

#[async_trait]
impl ConnectedPeripheral for BtleplugPeripheral {
    async fn discover_services(&self) -> Result<ServiceSummary> {
        self.peripheral
            .discover_services()
            .await
            .context("failed to discover services")?;

        let services = self.peripheral.services();
        let characteristics = self.peripheral.characteristics();
        let uart_rx = characteristics.iter().find(|ch| ch.uuid == UART_RX_UUID);
        let uart_tx = characteristics.iter().find(|ch| ch.uuid == UART_TX_UUID);
        let probe_status = characteristics
            .iter()
            .find(|ch| ch.uuid == PROBE_STATUS_CHAR_UUID);

        Ok(ServiceSummary {
            has_uart_service: services
                .iter()
                .any(|service| service.uuid == UART_SERVICE_UUID),
            has_uart_rx: uart_rx.is_some(),
            has_uart_tx: uart_tx.is_some(),
            has_probe_status_service: services
                .iter()
                .any(|service| service.uuid == PROBE_STATUS_SERVICE_UUID),
            has_probe_status_char: probe_status.is_some(),
            uart_rx_properties: uart_rx.map(|ch| format!("{:?}", ch.properties)),
            uart_tx_properties: uart_tx.map(|ch| format!("{:?}", ch.properties)),
            probe_status_properties: probe_status.map(|ch| format!("{:?}", ch.properties)),
        })
    }

    async fn read_device_info(&self) -> Result<DeviceInfo> {
        let characteristics = self.peripheral.characteristics();
        Ok(DeviceInfo {
            manufacturer: read_text_characteristic(
                &self.peripheral,
                &characteristics,
                MANUFACTURER_NAME_UUID,
            )
            .await,
            serial: read_text_characteristic(
                &self.peripheral,
                &characteristics,
                SERIAL_NUMBER_UUID,
            )
            .await,
            hardware_revision: read_text_characteristic(
                &self.peripheral,
                &characteristics,
                HARDWARE_REVISION_UUID,
            )
            .await,
            firmware_revision: read_text_characteristic(
                &self.peripheral,
                &characteristics,
                FIRMWARE_REVISION_UUID,
            )
            .await,
        })
    }

    async fn write_uart(&self, payload: &[u8], mode: WriteMode) -> Result<()> {
        let characteristics = self.peripheral.characteristics();
        let uart_rx = characteristics
            .iter()
            .find(|ch| ch.uuid == UART_RX_UUID)
            .ok_or_else(|| anyhow!("UART RX characteristic not found"))?;

        let write_type = match mode {
            WriteMode::Auto => choose_write_type(uart_rx.properties),
        };

        self.peripheral
            .write(uart_rx, payload, write_type)
            .await
            .context("failed to write UART RX payload")
    }

    async fn listen_notifications(&self, duration: Duration) -> Result<Vec<NotificationEvent>> {
        let characteristics = self.peripheral.characteristics();
        let uart_tx = characteristics.iter().find(|ch| ch.uuid == UART_TX_UUID);
        let probe_status = characteristics
            .iter()
            .find(|ch| ch.uuid == PROBE_STATUS_CHAR_UUID);

        let mut subscribed = Vec::new();
        if let Some(ch) = uart_tx {
            if ch.properties.contains(CharPropFlags::NOTIFY) {
                self.peripheral
                    .subscribe(ch)
                    .await
                    .context("failed to subscribe to UART TX")?;
                subscribed.push((NotificationSource::UartTx, ch.uuid));
            }
        }
        if let Some(ch) = probe_status {
            if ch.properties.contains(CharPropFlags::NOTIFY) {
                self.peripheral
                    .subscribe(ch)
                    .await
                    .context("failed to subscribe to Probe Status")?;
                subscribed.push((NotificationSource::ProbeStatus, ch.uuid));
            }
        }

        if subscribed.is_empty() {
            return Ok(Vec::new());
        }

        let mut stream = self
            .peripheral
            .notifications()
            .await
            .context("failed to open notification stream")?;
        let deadline = Instant::now() + duration;
        let mut notifications = Vec::new();

        loop {
            let remaining = match deadline.checked_duration_since(Instant::now()) {
                Some(remaining) => remaining,
                None => break,
            };

            match tokio::time::timeout(remaining, stream.next()).await {
                Ok(Some(notification)) => {
                    let source = if notification.uuid == UART_TX_UUID {
                        NotificationSource::UartTx
                    } else if notification.uuid == PROBE_STATUS_CHAR_UUID {
                        NotificationSource::ProbeStatus
                    } else {
                        continue;
                    };

                    notifications.push(NotificationEvent {
                        source,
                        characteristic_uuid: notification.uuid,
                        payload: notification.value,
                    });
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        Ok(notifications)
    }

    async fn disconnect(&self) -> Result<()> {
        self.peripheral
            .disconnect()
            .await
            .context("failed to disconnect cleanly")
    }
}

async fn snapshot_peripheral(peripheral: Peripheral) -> Result<Option<DiscoveryEvent>> {
    let peripheral_handle = format!("{:?}", peripheral.id());
    let properties = match peripheral
        .properties()
        .await
        .context("failed to fetch peripheral properties")?
    {
        Some(properties) => properties,
        None => return Ok(None),
    };

    let Some(payload) = properties.manufacturer_data.get(&COMBUSTION_COMPANY_ID) else {
        return Ok(None);
    };

    let discovery = parse_combustion_advertisement(payload).map(
        |(advertisement_family, product_type, serial_number)| DiscoveryEvent {
            peripheral_handle,
            local_name: properties.local_name,
            rssi: properties.rssi,
            advertisement_family,
            product_type,
            serial_number,
            raw_manufacturer_data: payload.clone(),
        },
    );

    Ok(discovery)
}

fn parse_combustion_advertisement(
    payload: &[u8],
) -> Option<(AdvertisementFamily, ProductType, String)> {
    if payload.is_empty() {
        return None;
    }

    let product_type = ProductType::from_byte(payload[0]);
    let family = classify_advertisement_family(product_type, payload.len())?;
    let serial_number = match family {
        AdvertisementFamily::DirectProbe | AdvertisementFamily::NodeRepeatedProbe => {
            if payload.len() < 5 {
                return None;
            }
            let serial = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]);
            format!("{serial:08X}")
        }
        AdvertisementFamily::NodeSelf => {
            if payload.len() < 11 {
                return None;
            }
            decode_text(&payload[1..11])?
        }
    };

    Some((family, product_type, serial_number))
}

fn classify_advertisement_family(
    product_type: ProductType,
    payload_len: usize,
) -> Option<AdvertisementFamily> {
    match (product_type, payload_len) {
        (ProductType::PredictiveProbe, 23) => Some(AdvertisementFamily::DirectProbe),
        (ProductType::PredictiveProbe, 22) => Some(AdvertisementFamily::NodeRepeatedProbe),
        (
            ProductType::MeatNetRepeater
            | ProductType::GiantGrillGauge
            | ProductType::Display
            | ProductType::Booster,
            len,
        ) if len >= 11 => Some(AdvertisementFamily::NodeSelf),
        _ => None,
    }
}

async fn read_text_characteristic(
    peripheral: &Peripheral,
    characteristics: &BTreeSet<btleplug::api::Characteristic>,
    uuid: uuid::Uuid,
) -> Option<String> {
    let characteristic = characteristics.iter().find(|characteristic| {
        characteristic.uuid == uuid && characteristic.properties.contains(CharPropFlags::READ)
    })?;

    let bytes = peripheral.read(characteristic).await.ok()?;
    decode_text(&bytes)
}

fn choose_write_type(properties: CharPropFlags) -> WriteType {
    if properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    }
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches('\0')
        .to_string();
    if text.is_empty() { None } else { Some(text) }
}

pub fn normalize_serial_input(product_type: ProductType, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("serial must not be empty");
    }

    if product_type.is_probe() {
        let without_prefix = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);
        let value = u32::from_str_radix(without_prefix, 16)
            .with_context(|| format!("invalid probe serial '{raw}'"))?;
        Ok(format!("{value:08X}"))
    } else {
        Ok(trimmed.trim_end_matches('\0').to_string())
    }
}
