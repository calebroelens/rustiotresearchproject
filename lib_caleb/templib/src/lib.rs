pub mod c_device;
pub mod c_enums;

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;
    use mcp3xxx::Channel;
    use rppal::spi;
    use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
    use crate::c_device::c_device::TemperatureSensor;
    #[test]
    pub fn spi_test(){
        let mut spi_m = rppal::spi::Spi::new(Bus::Spi0,
                                             SlaveSelect::Ss0,
                                             1350000,
                                             Mode::Mode0).unwrap();
        let mut spi_interface = mcp3xxx::Mcp3008::new(spi_m).unwrap();
        let mut channel = Channel::new(7).unwrap();
        let mut read_out = spi_interface.single_ended_read(channel);
        let result = read_out.unwrap();
        let value = result.value();
        println!("Value: {}", value);
    }
}