pub mod c_device {
    use std::fmt::{Display, Formatter};
    use rppal::i2c::I2c;
    use crate::c_enums::cs811_enums::{AppBootLoaderActions, MeasurementModes, MeasureRegister, Registers};

    pub struct CS811 {
        name: String,
        address: u8,
        measurement_mode: MeasurementModes,
    }
    impl CS811{

        pub fn new(name: String, address: u8) -> CS811{
            CS811{ name, address, measurement_mode: MeasurementModes::Idle }
        }

        pub fn setup(&self) -> I2c{
            let mut i2c_device = I2c::new().unwrap();
            i2c_device.set_slave_address(self.address as u16);
            i2c_device
        }
        pub fn set_measurement_mode(&mut self, ic2_device: &mut I2c, measurement_mode: MeasurementModes) {
            let value = measurement_mode.value();
            let measurement = MeasureRegister{
                threshold: 0,
                dataready: 0,
                drivemode: value
            };
            let write_value : u8 = measurement.into();
            println!("Write value: {}", write_value);
            let result = ic2_device.block_write(Registers::MeasurementMode.value(), &[write_value]);
            self.measurement_mode = measurement_mode;
        }
        pub fn get_device_id(&mut self, i2c_device: &mut I2c) -> u8{
            let mut buffer = [0 as u8; 1];
            let data = i2c_device.block_read(Registers::HardwareID.value(), &mut buffer);
            buffer[0]
        }
        pub fn get_device_co2(&mut self, i2c_device: &mut I2c) -> u16{
            let mut buffer = [0 as u8; 2];
            let data = i2c_device.block_read(Registers::AlgorithmicResultData.value(), &mut buffer);
            let number = CS811::two_byte_to_one(buffer);
            number
        }
        pub fn get_full_data_read(&mut self, i2c_device: &mut I2c) -> [u8; 5] {
            let mut buffer = [0 as u8; 5];
            let data = i2c_device.block_read(Registers::AlgorithmicResultData.value(), &mut buffer);
            buffer
        }
        pub fn read_full_buffer(&mut self, i2c_device: &mut I2c) -> Vec<u8>{
            let mut buffer = vec![0; 32];
            let data = i2c_device.read(&mut buffer);
            buffer
        }
        pub fn write_read(&mut self, i2c_device: &mut I2c, command: u8) -> Vec<u8>{
            let mut buffer = vec![0;32];
            let data = i2c_device.write_read(&[2],&mut buffer);
            buffer
        }
        pub fn enter_application_mode(&mut self, i2c_device: &mut I2c){
            i2c_device.write(&[AppBootLoaderActions::Start.value()]);
        }

        pub fn get_ntc_values(&mut self, i2c_device: &mut I2c) -> [u16; 2] {
            let mut buffer = [0; 4];
            i2c_device.block_read(Registers::NtcResistor.value(), &mut buffer);

            let mut c1 = CS811::two_byte_to_one([buffer[0],buffer[1]]);
            let mut c2 = CS811::two_byte_to_one([buffer[2],buffer[3]]);
            [c1,c2]
        }

        fn two_byte_to_one(buffer: [u8;2]) -> u16{
            ((buffer[0] as u16) << 8) | buffer[1] as u16
        }
    }
    impl Display for CS811{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "Sensor CS811 with name {} and address {}", self.name, self.address)
        }
    }
}