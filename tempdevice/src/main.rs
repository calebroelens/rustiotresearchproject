extern crate core;

use std::str::FromStr;
use std::thread;
use std::time::{Duration, SystemTime};
use amqpiothubv2;
use amqpiothubv2::amqp::transfer::{create_message_from_str, TransferExceptions};
use templib;
use amqpiothubv2::ntex;
use amqpiothubv2::ntex_amqp;
use amqpiothubv2::ntex_amqp::codec::protocol::{Transfer, TransferBody};
use rppal::gpio::Gpio;
use serde::{Serialize,Deserialize};
use serde_json;
use serde_json::Value;

//  This program should run on the raspberry pi with the air quality sensor.
#[derive(Serialize, Deserialize)]
struct DataEntry{
    sensor: String,
    value: f64
}

#[ntex::main]
async fn main() {
    // Main thread --> Apply ntex instance
    // Enable trace logs
    amqpiothubv2::amqp::util::enable_logging_traces(None);
    // Device Params
    let device_id =  "temperature";
    let primary_key = "";
    let hub_name = "";
    let cert_location = "src/root.pem";
    // Sas token
    let mut sas_token = amqpiothubv2::util::token::SasToken::new(
        &primary_key, 1, &hub_name, &device_id
    );
    // Check if token is valid.
    let mut sas_token_opt = match sas_token{
        Ok(ok) => {
            // Continue
            Some(ok)
        }
        Err(fail) => {
            // Failed to create the token --> Exit
            panic!("Failed SAS: {}",fail);
            None
        }
    };
    // Unwrap the token
    let mut token = sas_token_opt.unwrap();
    // Create client
    let mut client = amqpiothubv2::amqp::client::Client::new(
        &device_id,
        &hub_name,
        &cert_location,
        &primary_key,
        &token.sas)
        .await;
    // Check client state.
    let connectors = client.connect().await;
    let connectors_result = match connectors{
        Ok(connector) => {
            Some(connector)
        }
        Err(amqp_failure) => {
            // Failure -> Exit
            panic!("Failure on connect: {}", amqp_failure);
        }
    };
    let create_result = client.attach_sender(
        "sender_link_global",
        "/devices/temperature/messages/events",
        5
    ).await;
    /*
    match create_result{
        Ok(_) => {
            println!("Created");
        }
        Err(err) => {
            // Failed to create the sender.
            println!("Failed: {}", err);
            panic!("Initial sender create failure.");
        }
    }
    */
    let mut recv_response = client.attach_receiver(
        "recv_link_global",
        "devices/temperature/messages/devicebound",
        5)
        .await;

    // Create the sensor
    let mut temp_sensor = templib::c_device::c_device::TempSensor::new();

    let mut loop_time = SystemTime::now();

    loop{
        let mut sensor_value = match temp_sensor.read_sensor_raw(7){
            None => {
                0 as f64
            }
            Some(value) => {
                // Convert to a temperature
                let voltage = value as f64 / 1023.0 * 3.3;
                println!("Read voltage: {}", voltage);
                let result = ((voltage*1000.0 - 500.0) / 10.0) + 4.0;
                println!("Result: {}", result);
                result
            }
        };
        if sensor_value != 0.0{
            let prepare_payload = prepare_payload("temperature", sensor_value as f64);
            let send_msg_result = client.send_message(
                "sender_link_global",
                prepare_payload,
                10).await;
            match send_msg_result{
                Ok(_) => {
                    println!("Ok")
                }
                Err(e) => {
                    println!("Failed: {}", e);
                    // Failed to transfer a message
                    // Create new token:
                    client.recover().await;
                    client.reattach_sender_links().await;
                    client.reattach_receiver_links().await;
                }
            }
        }
        while loop_time.elapsed().unwrap().as_secs() < 20{
            let mut incoming_data = client.receive_message_listener(
                0, 2)
                .await;
            let has_data = match incoming_data{
                Ok(body) => {
                    Some(body)
                }
                Err(error) => {
                    println!("Transfer exception: {}", error);
                    None
                }
            };
            if has_data.is_some(){
                let content = message_handler(has_data.unwrap());
                if content.is_none(){
                    // Failed to read
                    println!("Failed to read the message contents");
                }
                else{
                    // There is some content
                    let read_content = content.unwrap();
                    let json: Value = serde_json::from_str(&read_content).unwrap();
                    // Attempt to get the action value
                    let action = json.get("action");
                    match action{
                        Some(action)  => {
                            println!("Found action: {}", action);
                            let action_val = action.as_str().unwrap();
                            if action_val == "test"{
                                println!("Led action");
                                let mut pin = Gpio::new().unwrap().get(26).unwrap().into_output();
                                pin.set_high();
                                let mut time = SystemTime::now();
                                thread::sleep(Duration::from_secs(2));
                                pin.set_low();
                            }
                        },
                        None => {
                            println!("Failed to get the action");
                        },
                    }

                }
            }
        }
        loop_time = SystemTime::now();
    }
}

fn prepare_payload(sensor_name: &str, value: f64) -> TransferBody {
    let data_entry = DataEntry{
        sensor: sensor_name.to_string(),
        value
    };
    let message_content_str = &format!("{}",serde_json::to_string(&data_entry)
        .unwrap())[..];
    let message_content = create_message_from_str(message_content_str);
    return message_content;
}

pub fn message_handler(transfer: Transfer) -> Option<String> {
    // Handle the message here.
    let mut body = transfer.body;
    if body.is_none(){
        return None;
    }
    let body_contents = body.unwrap();
    let str_contents = return match body_contents {
        TransferBody::Data(data) => {
            // The type is data.
            let sliced = data.clone();
            let bytes = sliced.iter().as_slice();
            let mut raw_string = String::new();
            for (index, char) in data.iter().enumerate() {
                match String::from_utf8(vec![*char]) {
                    Ok(ok) => {
                        raw_string.push_str(&ok);
                    }
                    Err(err) => {
                        continue;
                    }
                }
            }
            let mut json_string = String::new();
            for (mut index, mut char) in raw_string.chars().enumerate() {
                if char == char::from_str("{").unwrap() {
                    if raw_string.chars().nth(index + 1).unwrap() == char::from_str("\"").unwrap() {
                        let len = raw_string.len();
                        let slice = &raw_string[index..len];
                        json_string = slice.parse().unwrap();
                        break;
                    }
                }
            }
            Some(json_string)
        }
        TransferBody::Message(message) => {
            println!("Message type: {:?}", message);
            None
        }
    };

}
