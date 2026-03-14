mod ble;
mod discovery_cache;
mod types;

use std::future::Future;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

use ble::btleplug_backend::{BtleplugTransport, normalize_serial_input};
use ble::transport::{
    AdvertisementFamily, BleTransport, ConnectedPeripheral, DeviceInfo, DiscoveryEvent,
    NotificationEvent, NotificationSource, ServiceSummary, WriteMode,
};
use discovery_cache::{CachedDiscovery, load_recent_target, record_discoveries};
use types::ProductType;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(2);
const DISCOVERY_CACHE_MAX_AGE: Duration = Duration::from_secs(300);

#[derive(Debug, Parser)]
#[command(name = "sbc-service")]
#[command(about = "Phase 1 MeatNet BLE implementation for MacBook validation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Scan {
        #[arg(long, default_value_t = 6)]
        scan_seconds: u64,
    },
    Inspect {
        #[arg(long)]
        product_type: ProductType,
        #[arg(long)]
        serial: String,
        #[arg(long, default_value_t = 6)]
        scan_seconds: u64,
        #[arg(long, default_value_t = 10)]
        listen_seconds: u64,
        #[arg(long)]
        write_hex: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Command::Scan { scan_seconds } => scan(scan_seconds).await,
        Command::Inspect {
            product_type,
            serial,
            scan_seconds,
            listen_seconds,
            write_hex,
        } => {
            inspect(
                product_type,
                &serial,
                scan_seconds,
                listen_seconds,
                write_hex,
            )
            .await
        }
    }
}

async fn scan(scan_seconds: u64) -> Result<()> {
    let transport = BtleplugTransport::new_default().await?;
    println!("Scanning for {scan_seconds}s...");
    let discoveries =
        match await_with_shutdown_grace("scan", transport.scan(Duration::from_secs(scan_seconds)))
            .await?
        {
            OperationOutcome::Completed(discoveries) => discoveries,
            OperationOutcome::Interrupted(_) => {
                println!("Scan interrupted. Exiting cleanly.");
                return Ok(());
            }
        };

    record_discoveries(&discoveries)?;

    if discoveries.is_empty() {
        println!("No Combustion advertisements discovered.");
        return Ok(());
    }

    println!(
        "Discovered {} Combustion advertisements.",
        discoveries.len()
    );
    for discovery in discoveries {
        print_discovery(&discovery);
    }

    Ok(())
}

async fn inspect(
    product_type: ProductType,
    serial: &str,
    scan_seconds: u64,
    listen_seconds: u64,
    write_hex: Option<String>,
) -> Result<()> {
    let normalized_serial = normalize_serial_input(product_type, serial)?;
    let transport = BtleplugTransport::new_default().await?;

    println!(
        "Scanning for {}:{} for {scan_seconds}s...",
        product_type, normalized_serial
    );
    let discoveries =
        match await_with_shutdown_grace("scan", transport.scan(Duration::from_secs(scan_seconds)))
            .await?
        {
            OperationOutcome::Completed(discoveries) => discoveries,
            OperationOutcome::Interrupted(_) => {
                println!("Shutdown complete.");
                return Ok(());
            }
        };
    record_discoveries(&discoveries)?;
    let target = pick_target(&discoveries, product_type, &normalized_serial)?;

    println!("Selected target:");
    print_target(&target);

    let peripheral =
        match await_with_shutdown_grace("connect", transport.connect(target.peripheral_handle()))
            .await?
        {
            OperationOutcome::Completed(peripheral) => peripheral,
            OperationOutcome::Interrupted(Some(peripheral)) => {
                disconnect_with_report(peripheral.as_ref()).await;
                println!("Shutdown complete.");
                return Ok(());
            }
            OperationOutcome::Interrupted(None) => {
                println!("Shutdown complete.");
                return Ok(());
            }
        };
    let services =
        match await_with_shutdown_grace("service discovery", peripheral.discover_services()).await?
        {
            OperationOutcome::Completed(services) => services,
            OperationOutcome::Interrupted(_) => {
                disconnect_with_report(peripheral.as_ref()).await;
                println!("Shutdown complete.");
                return Ok(());
            }
        };
    let device_info =
        match await_with_shutdown_grace("device information read", peripheral.read_device_info())
            .await?
        {
            OperationOutcome::Completed(device_info) => device_info,
            OperationOutcome::Interrupted(_) => {
                disconnect_with_report(peripheral.as_ref()).await;
                println!("Shutdown complete.");
                return Ok(());
            }
        };
    confirm_connected_identity(&target, &device_info)?;

    println!("Validation summary:");
    print_service_summary(&services);
    print_device_info(&device_info);

    if let Some(write_hex) = write_hex {
        let payload = hex::decode(write_hex.trim())
            .with_context(|| format!("failed to decode --write-hex payload '{write_hex}'"))?;
        println!("Writing {} bytes to UART RX...", payload.len());
        match await_with_shutdown_grace(
            "UART write",
            peripheral.write_uart(&payload, WriteMode::Auto),
        )
        .await?
        {
            OperationOutcome::Completed(()) => {}
            OperationOutcome::Interrupted(_) => {
                disconnect_with_report(peripheral.as_ref()).await;
                println!("Shutdown complete.");
                return Ok(());
            }
        }
        println!("Write completed.");
    }

    let notifications = match await_with_shutdown_grace(
        "notification collection",
        peripheral.listen_notifications(Duration::from_secs(listen_seconds)),
    )
    .await?
    {
        OperationOutcome::Completed(notifications) => notifications,
        OperationOutcome::Interrupted(_) => {
            disconnect_with_report(peripheral.as_ref()).await;
            println!("Shutdown complete.");
            return Ok(());
        }
    };
    println!(
        "Received {} notifications over {}s.",
        notifications.len(),
        listen_seconds
    );
    for notification in notifications {
        print_notification(&notification);
    }

    peripheral.disconnect().await?;
    Ok(())
}

enum OperationOutcome<T> {
    Completed(T),
    Interrupted(Option<T>),
}

async fn await_with_shutdown_grace<F, T>(label: &str, future: F) -> Result<OperationOutcome<T>>
where
    F: Future<Output = Result<T>>,
{
    tokio::pin!(future);

    tokio::select! {
        result = &mut future => result.map(OperationOutcome::Completed),
        _ = tokio::signal::ctrl_c() => {
            println!(
                "Shutdown requested during {label}; waiting up to {}s for the current operation to settle...",
                SHUTDOWN_GRACE_PERIOD.as_secs()
            );

            match tokio::time::timeout(SHUTDOWN_GRACE_PERIOD, &mut future).await {
                Ok(Ok(value)) => {
                    println!("Current operation settled during shutdown.");
                    Ok(OperationOutcome::Interrupted(Some(value)))
                }
                Ok(Err(error)) => {
                    println!("Current operation ended with error during shutdown: {error:#}");
                    Ok(OperationOutcome::Interrupted(None))
                }
                Err(_) => {
                    println!("Grace period expired. Forcing shutdown.");
                    Ok(OperationOutcome::Interrupted(None))
                }
            }
        }
    }
}

async fn disconnect_with_report(peripheral: &dyn ConnectedPeripheral) {
    match peripheral.disconnect().await {
        Ok(()) => println!("Disconnected cleanly."),
        Err(error) => println!("Disconnect failed during shutdown: {error:#}"),
    }
}

fn pick_target<'a>(
    discoveries: &'a [DiscoveryEvent],
    product_type: ProductType,
    serial_number: &str,
) -> Result<TargetSelection<'a>> {
    let matching: Vec<_> = discoveries
        .iter()
        .filter(|discovery| {
            discovery.product_type == product_type && discovery.serial_number == serial_number
        })
        .collect();

    if product_type.is_probe() {
        if let Some(discovery) = matching
            .iter()
            .find(|discovery| discovery.advertisement_family == AdvertisementFamily::DirectProbe)
        {
            return Ok(TargetSelection::Fresh(discovery));
        }

        let repeated_probe_count = matching
            .iter()
            .filter(|discovery| {
                discovery.advertisement_family == AdvertisementFamily::NodeRepeatedProbe
            })
            .count();
        if repeated_probe_count > 0 {
            return Err(anyhow!(
                "saw probe identity only through node-repeated advertisements; no direct probe advertisement was available to connect to"
            ));
        }
    }

    if let Some(discovery) = matching.first() {
        return Ok(TargetSelection::Fresh(discovery));
    }

    if product_type.is_node() {
        if let Some(cached) =
            load_recent_target(product_type, serial_number, DISCOVERY_CACHE_MAX_AGE)?
        {
            return Ok(TargetSelection::Cached(cached));
        }
    }

    bail!("no advertisement matched {product_type}:{serial_number}");
}

enum TargetSelection<'a> {
    Fresh(&'a DiscoveryEvent),
    Cached(CachedDiscovery),
}

impl<'a> TargetSelection<'a> {
    fn peripheral_handle(&self) -> &str {
        match self {
            Self::Fresh(discovery) => &discovery.peripheral_handle,
            Self::Cached(discovery) => &discovery.peripheral_handle,
        }
    }

    fn product_type(&self) -> ProductType {
        match self {
            Self::Fresh(discovery) => discovery.product_type,
            Self::Cached(discovery) => discovery.product_type,
        }
    }

    fn serial_number(&self) -> &str {
        match self {
            Self::Fresh(discovery) => &discovery.serial_number,
            Self::Cached(discovery) => &discovery.serial_number,
        }
    }

    fn used_cache_fallback(&self) -> bool {
        matches!(self, Self::Cached(_))
    }
}

fn print_target(target: &TargetSelection<'_>) {
    match target {
        TargetSelection::Fresh(discovery) => print_discovery(discovery),
        TargetSelection::Cached(discovery) => {
            println!("handle={}", discovery.peripheral_handle);
            println!(
                "  key={}:{}",
                discovery.product_type, discovery.serial_number
            );
            println!("  family={}", discovery.advertisement_family.slug());
            println!("  source=cached-discovery");
        }
    }
}

fn confirm_connected_identity(
    target: &TargetSelection<'_>,
    device_info: &DeviceInfo,
) -> Result<()> {
    if !target.product_type().is_node() {
        return Ok(());
    }

    let connected_serial = device_info.serial.as_deref().ok_or_else(|| {
        anyhow!("connected node did not expose a GATT serial for identity confirmation")
    })?;

    if connected_serial != target.serial_number() {
        bail!(
            "connected node identity mismatch: expected {}:{}, got {}:{}",
            target.product_type(),
            target.serial_number(),
            target.product_type(),
            connected_serial
        );
    }

    if target.used_cache_fallback() {
        println!("Confirmed node identity from GATT serial after cached target fallback.");
    }

    Ok(())
}

fn print_discovery(discovery: &DiscoveryEvent) {
    println!("handle={}", discovery.peripheral_handle);
    println!(
        "  key={}:{}",
        discovery.product_type, discovery.serial_number
    );
    println!("  family={}", discovery.advertisement_family.slug());
    println!(
        "  local_name={}",
        discovery.local_name.as_deref().unwrap_or("(none)")
    );
    println!(
        "  rssi={}",
        discovery
            .rssi
            .map(|value| value.to_string())
            .unwrap_or_else(|| "(unknown)".to_string())
    );
    println!(
        "  payload_hex={}",
        hex::encode(&discovery.raw_manufacturer_data)
    );
}

fn print_service_summary(summary: &ServiceSummary) {
    println!(
        "  UART service present: {}",
        yes_no(summary.has_uart_service)
    );
    println!("  UART RX present:      {}", yes_no(summary.has_uart_rx));
    println!("  UART TX present:      {}", yes_no(summary.has_uart_tx));
    println!(
        "  Probe Status service: {}",
        yes_no(summary.has_probe_status_service)
    );
    println!(
        "  Probe Status char:    {}",
        yes_no(summary.has_probe_status_char)
    );
    if let Some(properties) = &summary.uart_rx_properties {
        println!("  UART RX properties:   {properties}");
    }
    if let Some(properties) = &summary.uart_tx_properties {
        println!("  UART TX properties:   {properties}");
    }
    if let Some(properties) = &summary.probe_status_properties {
        println!("  Probe Status props:   {properties}");
    }
}

fn print_device_info(device_info: &DeviceInfo) {
    println!("Device information:");
    println!(
        "  manufacturer: {}",
        device_info
            .manufacturer
            .as_deref()
            .unwrap_or("(unavailable)")
    );
    println!(
        "  serial:       {}",
        device_info.serial.as_deref().unwrap_or("(unavailable)")
    );
    println!(
        "  hardware:     {}",
        device_info
            .hardware_revision
            .as_deref()
            .unwrap_or("(unavailable)")
    );
    println!(
        "  firmware:     {}",
        device_info
            .firmware_revision
            .as_deref()
            .unwrap_or("(unavailable)")
    );
}

fn print_notification(notification: &NotificationEvent) {
    let source = match notification.source {
        NotificationSource::UartTx => "uart-tx",
        NotificationSource::ProbeStatus => "probe-status",
    };
    println!(
        "  notification source={} uuid={} bytes={}",
        source,
        notification.characteristic_uuid,
        hex::encode(&notification.payload)
    );
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
