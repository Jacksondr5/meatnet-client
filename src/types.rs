// trait MaxValue {
//     const MAX: i8;
// }

// struct SizedInt<T: MaxValue>(i8, std::marker::PhantomData<T>);

// impl<T: MaxValue> SizedInt<T> {
//     fn new(value: i8) -> Result<Self, &'static str> {
//         if value <= T::MAX {
//             Ok(SizedInt(value, std::marker::PhantomData))
//         } else {
//             Err("Value out of range for the specified size")
//         }
//     }

//     fn value(&self) -> i8 {
//         self.0
//     }
// }

// // Implement MaxValue for 3-bit size
// pub struct ThreeBit;

// impl MaxValue for ThreeBit {
//     const MAX: i8 = 7;
// }

// // Example usage
// fn main() {
//     match SizedInt::<ThreeBit>::new(5) {
//         Ok(u3) => println!("Valid U3 value: {}", u3.value()),
//         Err(e) => println!("Error: {}", e),
//     }

//     match SizedInt::<ThreeBit>::new(8) {
//         Ok(u3) => println!("Valid U3 value: {}", u3.value()),
//         Err(e) => println!("Error: {}", e),
//     }
// }

#[derive(Debug, PartialEq)]
pub enum ProductType {
    Unknown = 0,
    PredictiveProbe = 1,
    // Also used for the Repeater
    KitchenTimer = 2,
}

// #[derive(Debug)]
// pub enum ProbeMode {
//     Normal = 0,
//     InstantRead = 1,
//     Reserved = 2,
//     Error = 3,
// }

// #[derive(Debug)]
// pub enum Color {
//     Yellow = 0,
//     Grey = 1,
//     // 2-7 TBD
// }

// #[derive(Debug)]
// pub enum BatteryStatus {
//     Ok = 0,
//     Low = 1,
// }

pub struct ProbeAdvertisement {
    // pub vendor_id: String,
    pub product_type: ProductType,
    pub serial_number: String,
    // pub mode: ProbeMode,
    // pub color: Color,
    // pub id: ThreeBit,
    // pub battery_status: BatteryStatus,
}
