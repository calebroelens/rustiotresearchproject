
pub mod c_enums;
pub mod c_device;
pub mod c_util;


#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;
    use crate::c_enums::cs811_enums::{MeasurementModes, MeasureRegister};
    use crate::c_device::c_device::CS811;

    #[test]
    fn it_works() {
        let test = MeasureRegister{
            threshold: 0,
            dataready: 0,
            drivemode: MeasurementModes::Minute.value()
        };
        let bytes_array: Vec<u8> = test.into();
        println!("{:?}", bytes_array);

        let test = MeasureRegister{
            threshold: 0,
            dataready: 0,
            drivemode: MeasurementModes::Minute.value()
        };

        let byte: u8 = test.into();
        println!("{}", byte);
    }

    #[test]
    fn device(){
        let mut device_obj = CS811::new(String::from("CO Quality"), 0x5a);
        let mut driver = device_obj.setup();
        device_obj.enter_application_mode(&mut driver);
        let mut co2 = device_obj.get_device_co2(&mut driver);
        println!("Co²: {}", co2);
        let mut device_id = device_obj.get_device_id(&mut driver);
        println!("Device ID: {}", device_id);
        device_obj.set_measurement_mode(&mut driver, MeasurementModes::Second);
        loop{
            thread::sleep(Duration::from_millis(1000));
            let mut data = device_obj.get_device_co2(&mut driver);
            println!("Co²: {}", data);
            let mut temperature_voltage = device_obj.get_ntc_values(&mut driver);
            println!("Res 1 {} Res 2 {} ", temperature_voltage[0], temperature_voltage[1]);
        }
    }
}



