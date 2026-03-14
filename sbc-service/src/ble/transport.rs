use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::types::ProductType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvertisementFamily {
    DirectProbe,
    NodeRepeatedProbe,
    NodeSelf,
}

impl AdvertisementFamily {
    pub fn slug(self) -> &'static str {
        match self {
            Self::DirectProbe => "direct-probe",
            Self::NodeRepeatedProbe => "node-repeated-probe",
            Self::NodeSelf => "node-self",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveryEvent {
    pub peripheral_handle: String,
    pub local_name: Option<String>,
    pub rssi: Option<i16>,
    pub advertisement_family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
    pub raw_manufacturer_data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct ServiceSummary {
    pub has_uart_service: bool,
    pub has_uart_rx: bool,
    pub has_uart_tx: bool,
    pub has_probe_status_service: bool,
    pub has_probe_status_char: bool,
    pub uart_rx_properties: Option<String>,
    pub uart_tx_properties: Option<String>,
    pub probe_status_properties: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceInfo {
    pub manufacturer: Option<String>,
    pub serial: Option<String>,
    pub hardware_revision: Option<String>,
    pub firmware_revision: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationSource {
    UartTx,
    ProbeStatus,
}

#[derive(Clone, Debug)]
pub struct NotificationEvent {
    pub source: NotificationSource,
    pub characteristic_uuid: Uuid,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum WriteMode {
    Auto,
}

#[async_trait]
pub trait ConnectedPeripheral: Send + Sync {
    async fn discover_services(&self) -> Result<ServiceSummary>;
    async fn read_device_info(&self) -> Result<DeviceInfo>;
    async fn write_uart(&self, payload: &[u8], mode: WriteMode) -> Result<()>;
    async fn listen_notifications(&self, duration: Duration) -> Result<Vec<NotificationEvent>>;
    async fn disconnect(&self) -> Result<()>;
}

#[async_trait]
pub trait BleTransport: Send + Sync {
    async fn scan(&self, duration: Duration) -> Result<Vec<DiscoveryEvent>>;
    async fn connect(&self, peripheral_handle: &str) -> Result<Box<dyn ConnectedPeripheral>>;
}
