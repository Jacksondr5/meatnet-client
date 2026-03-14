use uuid::{Uuid, uuid};

pub const COMBUSTION_COMPANY_ID: u16 = 0x09C7;
pub const UART_SERVICE_UUID: Uuid = uuid!("6e400001-b5a3-f393-e0a9-e50e24dcca9e");
pub const UART_RX_UUID: Uuid = uuid!("6e400002-b5a3-f393-e0a9-e50e24dcca9e");
pub const UART_TX_UUID: Uuid = uuid!("6e400003-b5a3-f393-e0a9-e50e24dcca9e");
pub const PROBE_STATUS_SERVICE_UUID: Uuid = uuid!("00000100-caab-3792-3d44-97ae51c1407a");
pub const PROBE_STATUS_CHAR_UUID: Uuid = uuid!("00000101-caab-3792-3d44-97ae51c1407a");
pub const MANUFACTURER_NAME_UUID: Uuid = uuid!("00002a29-0000-1000-8000-00805f9b34fb");
pub const SERIAL_NUMBER_UUID: Uuid = uuid!("00002a25-0000-1000-8000-00805f9b34fb");
pub const HARDWARE_REVISION_UUID: Uuid = uuid!("00002a27-0000-1000-8000-00805f9b34fb");
pub const FIRMWARE_REVISION_UUID: Uuid = uuid!("00002a26-0000-1000-8000-00805f9b34fb");
