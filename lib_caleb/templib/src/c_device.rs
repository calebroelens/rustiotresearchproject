pub mod c_device {
    use std::fmt::{Display, Formatter};
    use std::fs::read;
    use mcp3xxx::{Channel, Mcp3008};
    use rppal::spi::{Bus, Mode, SlaveSelect};
    use crate::c_enums::c_enums::ReferenceMode;

    pub struct TempSensor {
        pub(crate) mcp_instance: Mcp3008,
    }

    impl TempSensor {
        pub fn new() -> TempSensor {
            // Create
            let spi_instance = rppal::spi::Spi
            ::new(Bus::Spi0,
                  SlaveSelect::Ss0,
                  1350000,
                  Mode::Mode0).unwrap();
            // Connect
            let mcp_instance = mcp3xxx::Mcp3008::new(spi_instance)
                .unwrap();
            TempSensor{
                mcp_instance,
            }
        }
        pub fn read_sensor_raw(&mut self, channel: u8) -> Option<u16> {
            let mut channel = Channel::new(channel);
            let mut reading =
                self.mcp_instance.single_ended_read(channel.unwrap());
            if reading.is_err(){
                // Failed to read the value
                return None
            }
            return Some(reading.unwrap().value());
        }
        pub fn convert_to_temperature(value: u16, voltage_ref: ReferenceMode) -> f64{
            let mut voltage_factor = match voltage_ref{
                ReferenceMode::Ref3V3 => 3.3,
                ReferenceMode::Ref5V => 5.0
            };
            let result =  ((voltage_factor * value as f64 * 100.0)/1023.0) - 46.0;
            return result;
        }
    }

    impl Display for TempSensor {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "")
        }
    }
}