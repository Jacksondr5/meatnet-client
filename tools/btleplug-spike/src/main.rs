use std::fmt;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use clap::{Parser, Subcommand, ValueEnum};
use futures::StreamExt;
use tokio::time::{Instant, sleep};
use uuid::{Uuid, uuid};

const COMBUSTION_COMPANY_ID: u16 = 0x09C7;
const UART_SERVICE_UUID: Uuid = uuid!("6e400001-b5a3-f393-e0a9-e50e24dcca9e");
const UART_RX_UUID: Uuid = uuid!("6e400002-b5a3-f393-e0a9-e50e24dcca9e");
const UART_TX_UUID: Uuid = uuid!("6e400003-b5a3-f393-e0a9-e50e24dcca9e");
const PROBE_STATUS_SERVICE_UUID: Uuid = uuid!("00000100-caab-3792-3d44-97ae51c1407a");
const PROBE_STATUS_CHAR_UUID: Uuid = uuid!("00000101-caab-3792-3d44-97ae51c1407a");
const MANUFACTURER_NAME_UUID: Uuid = uuid!("00002a29-0000-1000-8000-00805f9b34fb");
const SERIAL_NUMBER_UUID: Uuid = uuid!("00002a25-0000-1000-8000-00805f9b34fb");
const HARDWARE_REVISION_UUID: Uuid = uuid!("00002a27-0000-1000-8000-00805f9b34fb");
const FIRMWARE_REVISION_UUID: Uuid = uuid!("00002a26-0000-1000-8000-00805f9b34fb");

#[derive(Debug, Parser)]
#[command(name = "btleplug-spike")]
#[command(about = "Minimal MeatNet BLE validation spike using btleplug")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scan for nearby Combustion devices and print advertisement details.
    Scan {
        #[arg(long, default_value_t = 6)]
        scan_seconds: u64,
    },
    /// Connect to one device selected by canonical key and validate GATT access.
    Inspect {
        #[arg(long)]
        product_type: ProductType,
        #[arg(long, value_parser = parse_serial_arg)]
        serial: u32,
        #[arg(long, default_value_t = 6)]
        scan_seconds: u64,
        #[arg(long, default_value_t = 10)]
        listen_seconds: u64,
        #[arg(long)]
        write_hex: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
enum ProductType {
    Unknown,
    PredictiveProbe,
    MeatNetRepeater,
    GiantGrillGauge,
    Display,
    Booster,
}

impl ProductType {
    fn from_byte(raw: u8) -> Self {
        match raw {
            1 => Self::PredictiveProbe,
            2 => Self::MeatNetRepeater,
            3 => Self::GiantGrillGauge,
            4 => Self::Display,
            5 => Self::Booster,
            _ => Self::Unknown,
        }
    }

    fn as_key_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::PredictiveProbe => "predictive-probe",
            Self::MeatNetRepeater => "meatnet-repeater",
            Self::GiantGrillGauge => "giant-grill-gauge",
            Self::Display => "display",
            Self::Booster => "booster",
        }
    }

    fn is_probe(self) -> bool {
        matches!(self, Self::PredictiveProbe)
    }
}

impl fmt::Display for ProductType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdvertisementSource {
    DirectProbe,
    NodeRepeatedProbe,
    NodeDevice,
    UnknownCombustion,
}

impl fmt::Display for AdvertisementSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::DirectProbe => "direct-probe",
            Self::NodeRepeatedProbe => "node-repeated-probe",
            Self::NodeDevice => "node-device",
            Self::UnknownCombustion => "unknown-combustion",
        };
        f.write_str(value)
    }
}

#[derive(Clone, Debug)]
struct CombustionAdvertisement {
    product_type: ProductType,
    serial_number: u32,
    payload_len: usize,
    source: AdvertisementSource,
    raw_payload: Vec<u8>,
}

#[derive(Clone)]
struct PeripheralSnapshot {
    peripheral: Peripheral,
    handle: String,
    local_name: Option<String>,
    rssi: Option<i16>,
    combustion: Option<CombustionAdvertisement>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Scan { scan_seconds } => run_scan(scan_seconds).await,
        Command::Inspect {
            product_type,
            serial,
            scan_seconds,
            listen_seconds,
            write_hex,
        } => {
            run_inspect(
                product_type,
                serial,
                scan_seconds,
                listen_seconds,
                write_hex,
            )
            .await
        }
    }
}

async fn run_scan(scan_seconds: u64) -> Result<()> {
    let adapter = default_adapter().await?;
    println!("Scanning for {scan_seconds}s...");
    let snapshots = scan_for_peripherals(&adapter, scan_seconds).await?;

    if snapshots.is_empty() {
        println!("No peripherals discovered.");
        return Ok(());
    }

    let combustion_count = snapshots
        .iter()
        .filter(|snapshot| snapshot.combustion.is_some())
        .count();

    println!(
        "Discovered {} peripherals ({} Combustion candidates).",
        snapshots.len(),
        combustion_count
    );

    for snapshot in snapshots {
        print_snapshot(&snapshot);
    }

    Ok(())
}

async fn run_inspect(
    target_product_type: ProductType,
    target_serial: u32,
    scan_seconds: u64,
    listen_seconds: u64,
    write_hex: Option<String>,
) -> Result<()> {
    let adapter = default_adapter().await?;
    println!(
        "Scanning for {}:{} for {scan_seconds}s...",
        target_product_type,
        format_serial(target_serial)
    );

    let snapshots = scan_for_peripherals(&adapter, scan_seconds).await?;
    let target = pick_target(&snapshots, target_product_type, target_serial)?;

    println!("Selected target:");
    print_snapshot(&target);

    if target.peripheral.is_connected().await.unwrap_or(false) {
        println!("Peripheral is already connected.");
    } else {
        println!("Connecting...");
        target
            .peripheral
            .connect()
            .await
            .with_context(|| format!("failed to connect to handle {}", target.handle))?;
    }

    println!("Discovering services...");
    target
        .peripheral
        .discover_services()
        .await
        .context("failed to discover services")?;

    let services = target.peripheral.services();
    let characteristics = target.peripheral.characteristics();

    println!(
        "Discovered {} services and {} characteristics.",
        services.len(),
        characteristics.len()
    );

    let uart_service = services
        .iter()
        .find(|service| service.uuid == UART_SERVICE_UUID);
    let probe_status_service = services
        .iter()
        .find(|service| service.uuid == PROBE_STATUS_SERVICE_UUID);
    let uart_rx = characteristics.iter().find(|ch| ch.uuid == UART_RX_UUID);
    let uart_tx = characteristics.iter().find(|ch| ch.uuid == UART_TX_UUID);
    let probe_status = characteristics
        .iter()
        .find(|ch| ch.uuid == PROBE_STATUS_CHAR_UUID);

    println!("Validation summary:");
    println!("  UART service present: {}", yes_no(uart_service.is_some()));
    println!("  UART RX present:      {}", yes_no(uart_rx.is_some()));
    println!("  UART TX present:      {}", yes_no(uart_tx.is_some()));
    println!(
        "  Probe Status service: {}",
        yes_no(probe_status_service.is_some())
    );
    println!("  Probe Status char:    {}", yes_no(probe_status.is_some()));

    if let Some(ch) = uart_rx {
        println!("  UART RX properties:   {:?}", ch.properties);
    }
    if let Some(ch) = uart_tx {
        println!("  UART TX properties:   {:?}", ch.properties);
    }
    if let Some(ch) = probe_status {
        println!("  Probe Status props:   {:?}", ch.properties);
    }

    read_device_info(&target.peripheral, &characteristics).await;

    if let Some(hex_payload) = write_hex {
        let payload = hex::decode(hex_payload.trim())
            .context("failed to decode --write-hex payload as hex")?;
        let rx = uart_rx.context("cannot write: UART RX characteristic not found")?;
        let write_type = choose_write_type(rx.properties);
        println!(
            "Writing {} bytes to UART RX using {:?}...",
            payload.len(),
            write_type
        );
        target
            .peripheral
            .write(rx, &payload, write_type)
            .await
            .context("failed to write UART RX payload")?;
        println!("Write completed.");
    }

    listen_for_notifications(&target.peripheral, uart_tx, probe_status, listen_seconds).await?;

    println!("Disconnecting...");
    target
        .peripheral
        .disconnect()
        .await
        .context("failed to disconnect cleanly")?;

    Ok(())
}

async fn default_adapter() -> Result<Adapter> {
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
    println!("Using first available Bluetooth adapter.");
    Ok(adapter)
}

async fn scan_for_peripherals(
    adapter: &Adapter,
    scan_seconds: u64,
) -> Result<Vec<PeripheralSnapshot>> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .context("failed to start BLE scan")?;
    sleep(Duration::from_secs(scan_seconds)).await;

    let peripherals = adapter
        .peripherals()
        .await
        .context("failed to read discovered peripherals")?;

    let mut snapshots = Vec::with_capacity(peripherals.len());
    for peripheral in peripherals {
        if let Some(snapshot) = snapshot_peripheral(peripheral).await? {
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

async fn snapshot_peripheral(peripheral: Peripheral) -> Result<Option<PeripheralSnapshot>> {
    let handle = format!("{:?}", peripheral.id());
    let properties = match peripheral
        .properties()
        .await
        .context("failed to fetch peripheral properties")?
    {
        Some(properties) => properties,
        None => return Ok(None),
    };

    let combustion = properties
        .manufacturer_data
        .get(&COMBUSTION_COMPANY_ID)
        .and_then(|payload| parse_combustion_advertisement(payload));

    Ok(Some(PeripheralSnapshot {
        peripheral,
        handle,
        local_name: properties.local_name,
        rssi: properties.rssi,
        combustion,
    }))
}

fn parse_combustion_advertisement(payload: &[u8]) -> Option<CombustionAdvertisement> {
    if payload.len() < 5 {
        return None;
    }

    let product_type = ProductType::from_byte(payload[0]);
    let serial_number = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]);
    let source = classify_advertisement(product_type, payload.len());

    Some(CombustionAdvertisement {
        product_type,
        serial_number,
        payload_len: payload.len(),
        source,
        raw_payload: payload.to_vec(),
    })
}

fn classify_advertisement(product_type: ProductType, payload_len: usize) -> AdvertisementSource {
    match (product_type, payload_len) {
        // btleplug exposes manufacturer data without the 2-byte company ID.
        (ProductType::PredictiveProbe, 23) => AdvertisementSource::DirectProbe,
        (ProductType::PredictiveProbe, 22) => AdvertisementSource::NodeRepeatedProbe,
        (
            ProductType::MeatNetRepeater
            | ProductType::GiantGrillGauge
            | ProductType::Display
            | ProductType::Booster,
            _,
        ) => AdvertisementSource::NodeDevice,
        _ => AdvertisementSource::UnknownCombustion,
    }
}

fn pick_target(
    snapshots: &[PeripheralSnapshot],
    target_product_type: ProductType,
    target_serial: u32,
) -> Result<PeripheralSnapshot> {
    let matching: Vec<_> = snapshots
        .iter()
        .filter(|snapshot| {
            let Some(adv) = &snapshot.combustion else {
                return false;
            };
            adv.product_type == target_product_type && adv.serial_number == target_serial
        })
        .cloned()
        .collect();

    if matching.is_empty() {
        bail!(
            "no advertisement matched {}:{}",
            target_product_type,
            format_serial(target_serial)
        );
    }

    if target_product_type.is_probe() {
        if let Some(snapshot) = matching.iter().find(|snapshot| {
            snapshot.combustion.as_ref().map(|adv| adv.source)
                == Some(AdvertisementSource::DirectProbe)
        }) {
            return Ok(snapshot.clone());
        }

        let repeated_count = matching
            .iter()
            .filter(|snapshot| {
                snapshot.combustion.as_ref().map(|adv| adv.source)
                    == Some(AdvertisementSource::NodeRepeatedProbe)
            })
            .count();

        if repeated_count > 0 {
            bail!(
                "saw probe identity {}:{} only through node-repeated probe advertisements; no direct probe advertisement was available to connect to",
                target_product_type,
                format_serial(target_serial)
            );
        }
    }

    Ok(matching[0].clone())
}

async fn read_device_info(
    peripheral: &Peripheral,
    characteristics: &std::collections::BTreeSet<btleplug::api::Characteristic>,
) {
    println!("Device information reads:");
    for (label, uuid) in [
        ("manufacturer", MANUFACTURER_NAME_UUID),
        ("serial", SERIAL_NUMBER_UUID),
        ("hardware", HARDWARE_REVISION_UUID),
        ("firmware", FIRMWARE_REVISION_UUID),
    ] {
        match characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == uuid)
        {
            Some(characteristic) if characteristic.properties.contains(CharPropFlags::READ) => {
                match peripheral.read(characteristic).await {
                    Ok(bytes) => println!("  {label}: {}", decode_text_or_hex(&bytes)),
                    Err(error) => println!("  {label}: read failed ({error})"),
                }
            }
            Some(_) => println!("  {label}: characteristic present but not readable"),
            None => println!("  {label}: characteristic not present"),
        }
    }
}

async fn listen_for_notifications(
    peripheral: &Peripheral,
    uart_tx: Option<&btleplug::api::Characteristic>,
    probe_status: Option<&btleplug::api::Characteristic>,
    listen_seconds: u64,
) -> Result<()> {
    let mut subscribed = Vec::new();

    if let Some(characteristic) = uart_tx {
        if characteristic.properties.contains(CharPropFlags::NOTIFY) {
            peripheral
                .subscribe(characteristic)
                .await
                .context("failed to subscribe to UART TX")?;
            subscribed.push(("uart-tx", characteristic.uuid));
        }
    }

    if let Some(characteristic) = probe_status {
        if characteristic.properties.contains(CharPropFlags::NOTIFY) {
            peripheral
                .subscribe(characteristic)
                .await
                .context("failed to subscribe to Probe Status")?;
            subscribed.push(("probe-status", characteristic.uuid));
        }
    }

    if subscribed.is_empty() {
        println!("No notifiable characteristics selected.");
        return Ok(());
    }

    println!("Subscribed to:");
    for (label, uuid) in &subscribed {
        println!("  {label}: {uuid}");
    }

    let mut stream = peripheral
        .notifications()
        .await
        .context("failed to open notification stream")?;
    let deadline = Instant::now() + Duration::from_secs(listen_seconds);
    let mut total_notifications = 0usize;

    println!("Listening for notifications for {listen_seconds}s...");
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline - now;
        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(notification)) => {
                total_notifications += 1;
                println!(
                    "  notification {} uuid={} len={} data={}",
                    total_notifications,
                    notification.uuid,
                    notification.value.len(),
                    hex::encode(&notification.value)
                );
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    println!("Received {total_notifications} notifications.");
    Ok(())
}

fn choose_write_type(properties: CharPropFlags) -> WriteType {
    if properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    }
}

fn print_snapshot(snapshot: &PeripheralSnapshot) {
    println!("handle={}", snapshot.handle);
    println!(
        "  local_name={}",
        snapshot.local_name.as_deref().unwrap_or("(none)")
    );
    println!(
        "  rssi={}",
        snapshot
            .rssi
            .map(|value| value.to_string())
            .unwrap_or_else(|| "(unknown)".to_string())
    );

    match &snapshot.combustion {
        Some(adv) => {
            println!(
                "  combustion_key={}:{}",
                adv.product_type,
                format_serial(adv.serial_number)
            );
            println!("  combustion_source={}", adv.source);
            println!("  payload_len={}", adv.payload_len);
            println!("  payload_hex={}", hex::encode(&adv.raw_payload));
        }
        None => println!("  combustion=none"),
    }
}

fn decode_text_or_hex(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim_end_matches('\0').to_string(),
        Err(_) => format!("0x{}", hex::encode(bytes)),
    }
}

fn parse_serial_arg(raw: &str) -> Result<u32, String> {
    let trimmed = raw.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(without_prefix, 16)
        .map_err(|error| format!("invalid hex serial '{raw}': {error}"))
}

fn format_serial(serial: u32) -> String {
    format!("{serial:08X}")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
