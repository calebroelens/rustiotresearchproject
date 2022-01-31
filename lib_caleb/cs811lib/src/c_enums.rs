pub mod cs811_enums {
    use std::fmt::{Display, Formatter};

    pub enum MeasurementModes {
        Idle,           // No measurement
        Second,         // Once a second
        TenSeconds,     // Every ten seconds
        Minute,         // Every minute
        Fast,           // Every 10ms
    }
    impl MeasurementModes {
        pub fn value(&self) -> u8{
            match *self{
                MeasurementModes::Idle => 0x00,
                MeasurementModes::Second => 0x01,
                MeasurementModes::TenSeconds => 0x02,
                MeasurementModes::Minute => 0x03,
                MeasurementModes::Fast => 0x04,
            }
        }
    }
    impl Display for MeasurementModes{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match &self{
                MeasurementModes::Idle => write!(f, "Idle"),
                MeasurementModes::Second => write!(f, "Second"),
                MeasurementModes::TenSeconds => write!(f, "Ten seconds"),
                MeasurementModes::Minute => write!(f, "Minute"),
                MeasurementModes::Fast => write!(f, "Fast"),

            }
        }
    }
    pub enum AppBootLoaderActions {
        Erase,
        Data,
        Verify,
        Start,
    }
    impl AppBootLoaderActions{
        pub fn value(&self) -> u8{
            match *self{
                AppBootLoaderActions::Erase => 0xF1,
                AppBootLoaderActions::Data => 0xF2,
                AppBootLoaderActions::Verify => 0xF3,
                AppBootLoaderActions::Start => 0xF4,
            }
        }
    }
    impl Display for AppBootLoaderActions{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match &self{
                AppBootLoaderActions::Erase => write!(f, "Erase"),
                AppBootLoaderActions::Data => write!(f, "Data"),
                AppBootLoaderActions::Verify => write!(f, "Verify"),
                AppBootLoaderActions::Start => write!(f, "Start"),
            }
        }
    }

    pub enum Registers{
        Status,
        MeasurementMode,
        AlgorithmicResultData,
        RawData,
        EnvironmentData,
        NtcResistor,
        Thresholds,
        BaseLine,
        HardwareID,
        HardwareVersion,
        FirmwareBootVersion,
        FirmwareApplicationVersion,
        ErrorID,
        SoftReset,
    }

    impl Registers {
        pub fn value(&self) -> u8{
            match *self{
                Registers::Status => {0x00}
                Registers::MeasurementMode => {0x01}
                Registers::AlgorithmicResultData => {0x02}
                Registers::RawData => {0x03}
                Registers::EnvironmentData => {0x05}
                Registers::NtcResistor => {0x06}
                Registers::Thresholds => {0x10}
                Registers::BaseLine => {0x11}
                Registers::HardwareID => {0x20}
                Registers::HardwareVersion => {0x21}
                Registers::FirmwareBootVersion => {0x23}
                Registers::FirmwareApplicationVersion => {0x24}
                Registers::ErrorID => {0xE0}
                Registers::SoftReset => {0xFF}
            }
        }
    }

    pub struct MeasureRegister{
        pub(crate) threshold: u8,
        pub(crate) dataready: u8,
        pub(crate) drivemode: u8
    }

    impl Into<Vec<u8>> for MeasureRegister{
        fn into(self) -> Vec<u8> {
            let mut result = vec![0 as u8; 8];
            result[6] = match self.drivemode{
                0x04 => 1,
                _ => 0
            };
            result[5] = match self.drivemode{
                0x02 => 1,
                0x03 => 1,
                _ => 0
            };
            result[4] = match self.drivemode{
                0x01 => 1,
                0x03 => 1,
                _ => 0
            };
            result[3] = self.dataready;
            result[2] = self.threshold;

            result
        }
    }

    impl Into<u8> for MeasureRegister {
        fn into(self) -> u8 {
            let mut total = 0;
            total += self.drivemode * 16;
            total += self.dataready * 8;
            total += self.threshold * 4;
            total
        }
    }
}