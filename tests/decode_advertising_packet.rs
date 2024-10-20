#[path = "../src/decode_advertising_packet.rs"]
mod decode_advertising_packet;
#[path = "../src/types.rs"]
mod types;

use crate::types::{Color, ProbeMode, ProductType, SizedInt, ThreeBit};

#[cfg(test)]
mod tests {
    use super::*;
    // use decodeAdvertisingPacket::decodeAdvertisingPacket;
    // use decodeAdvertisingPacket; // Import the function from the appropriate module
    use decode_advertising_packet::decode_advertising_packet;

    const DEFAULT_PACKET: [u8; 22] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn should_decode_product_type() {
        let mut packet = DEFAULT_PACKET.to_vec();

        // Predictive probe
        packet[0] = 0x01;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(
            probe_advertisement.product_type,
            ProductType::PredictiveProbe
        );

        // Kitchen timer
        packet[0] = 0x02;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.product_type, ProductType::KitchenTimer);

        // Unknown
        packet[0] = 0x00;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.product_type, ProductType::Unknown);

        // Invalid product type
        packet[0] = 0x03;
        assert!(std::panic::catch_unwind(|| decode_advertising_packet(&packet)).is_err());
    }

    #[test]
    fn should_decode_serial_number() {
        let mut packet = DEFAULT_PACKET.to_vec();
        packet[1..5].copy_from_slice(&[0x05, 0x52, 0x00, 0x10]);
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.serial_number, "10005205");
    }

    #[test]
    fn should_decode_mode() {
        let mut packet = DEFAULT_PACKET.to_vec();

        // Normal
        packet[18] = 0x00;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.mode, ProbeMode::Normal);

        // Instant read
        packet[18] = 0x01;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.mode, ProbeMode::InstantRead);

        // Reserved
        packet[18] = 0x02;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.mode, ProbeMode::Reserved);

        // Error
        packet[18] = 0x03;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.mode, ProbeMode::Error);
    }

    #[test]
    fn should_decode_color() {
        let mut packet = DEFAULT_PACKET.to_vec();

        // Yellow
        packet[18] = 0b0000_0000;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.color, Color::Yellow);

        // Grey
        packet[18] = 0b0000_0100;
        let probe_advertisement = decode_advertising_packet(&packet);
        assert_eq!(probe_advertisement.color, Color::Grey);

        // Invalid color
        packet[18] = 0b0000_1000;
        assert!(std::panic::catch_unwind(|| decode_advertising_packet(&packet)).is_err());
    }

    #[test]
    fn should_decode_id() {
        let mut packet = DEFAULT_PACKET.to_vec();
        packet[18] = 0b0010_0000;
        let probe_advertisement = decode_advertising_packet(&packet);
        // Create new ThreeBit
        let id = SizedInt::<ThreeBit>::new(1).expect("Somehow this is invalid");
        assert_eq!(probe_advertisement.id, id);
    }
}
