use crate::types::{Color, ProbeAdvertisement, ProbeMode, ProductType, SizedInt, ThreeBit};

pub fn decode_advertising_packet(packet: &Vec<u8>) -> ProbeAdvertisement {
    // Decode the product type (byte 0)
    let product_type_byte = packet[0];
    let product_type = match product_type_byte {
        0 => ProductType::Unknown,
        1 => ProductType::PredictiveProbe,
        2 => ProductType::KitchenTimer,
        _ => panic!("Unknown product type: {}", product_type_byte),
    };

    // Decode the serial number (bytes 1-4)
    // Serial number is a little-endian packed bitfield)
    let serial_number = packet[1..5]
        .iter()
        .map(|byte| format!("{:02X}", byte))
        // Reverse because little-endian
        .rev()
        .collect::<String>();

    // Decode the mode, color, and ID (byte 18)
    let mode_color_id_byte = packet[18];
    // The mode is bits 0-1 of mode_color_id_byte
    let mode_bits = mode_color_id_byte & 0b0000_0011;
    let mode = match mode_bits {
        0 => ProbeMode::Normal,
        1 => ProbeMode::InstantRead,
        2 => ProbeMode::Reserved,
        3 => ProbeMode::Error,
        _ => panic!("Unknown mode: {}, this should be unreachable", mode_bits),
    };

    // The color is bits 2-4 of mode_color_id_byte
    let color_bits = (mode_color_id_byte & 0b0001_1100) >> 2;
    let color = match color_bits {
        0 => Color::Yellow,
        1 => Color::Grey,
        _ => panic!("Unknown color: {}", color_bits),
    };

    // The ID is bits 5-7 of mode_color_id_byte

    // Return
    ProbeAdvertisement {
        color,
        id: SizedInt::<ThreeBit>::new(mode_color_id_byte >> 5).unwrap(),
        mode,
        product_type,
        serial_number,
    }
}
