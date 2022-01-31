use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, mpsc};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use amqpiothubv2;
use amqpiothubv2::{amqp, ntex_amqp};
use amqpiothubv2::amqp::transfer::{create_directed_message, create_message_from_str};
use amqpiothubv2::amqp::util;
use amqpiothubv2::async_std;
use amqpiothubv2::ntex;
use amqpiothubv2::ntex_amqp::types::Transfer;
use amqpiothubv2::util::token::SasToken;
use azureblobmanager;
use azureblobmanager::Blob;
use azureblobmanager::storage::{BlobPlotData, StorageEntry, StorageEntryFields, StorageReadClient};
use chrono::{DateTime, TimeZone, Utc};
use rocket;
use rocket::{fairing, get, main, routes, State};
use rocket::fs::NamedFile;
use rocket::http::Status;
use rocket_dyn_templates;
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio;
use ping;
use rand;
use serde_json::Value;


use crate::async_std::sync::Mutex;
use crate::ntex::web::ws::start;
use crate::ntex_amqp::codec::protocol::TransferBody;

#[derive(Serialize, Deserialize)]
struct DataEntry{
    sensor: String,
    value: f64
}

#[derive(Serialize, Deserialize)]
struct Action{
    action: String
}

#[get("/device_actions/<device_id>")]
async fn devices(device_id: &str) -> String{
    println!("Sender triggered... Preparing payloads...");
    let target_path = match device_id{
        "airquality" =>  {
            String::from("/devices/airquality/messages/devicebound")
        }
        "temperature" => {
            String::from("/devices/temperature/messages/devicebound")
        }
        _ => {
            String::from("/devices/rusttestingdevice/messages/devicebound")
        }
    };
    let action = Action{
        action: "test".to_string()
    };
    let mut device = String::from(device_id);
    let json = serde_json::to_string(&action);
    let mut message = create_directed_message(json.unwrap(), target_path);

    tokio::task::spawn_blocking(|| {
        ntex_main(message, device);
    }).await.expect("Task panicked");
    return String::from("Executed");
}

#[get("/device_data/<device_id>/<count>")]
async fn device_data(device_id: &str, count: u64) -> String
{
    let raw_data = get_blob_urls().await;
    return String::new();
}

#[get("/device_data_vars/date")]
async fn get_available_dates() -> String {
    let get_blobs = get_blob_urls().await;
    let blobs = get_blobs.0;
    let client = get_blobs.1;
    let mut result_dates = Vec::<DateTime<Utc>>::new();
    for blob in blobs{
        let date = blob.properties.last_modified;
        if !result_dates.iter().any(| &i | i.date() == date.date()){
            result_dates.push(date);
        }
    }
    let json = serde_json::to_string(&result_dates);
    return json.unwrap();
}

#[get("/device_data/<sensor_id>/<year>/<month>/<day>")]
async fn device_data_period(sensor_id: &str, year: i32, month: u32, day: u32) -> String {
    let date_time = Utc.ymd(year, month, day).and_hms(0,0,0);
    let get_blob_urls = get_blob_urls().await;
    let all_blobs = get_blob_urls.0;
    let mut client =  get_blob_urls.1;
    let result = StorageReadClient::collect_all_blobs_day(all_blobs, date_time);
    let future_result = result.await;
    // Fetch the data
    let response = fetch_device_data(future_result, client).await;
    println!("Total collected values: {}", response.len());
    // Read the contents of the values
    let json_string = String::new();
    let mut plot_data = Vec::<BlobPlotData>::new();
    let mut counter = 0;
    for data in response{
        counter += 1;
        println!("Reading value {}: {}", counter-1, data);
        let entry = StorageEntry::new(data);
        let contents = entry.get_data_as_vec();
        for (content_index, content) in contents.iter().enumerate(){
            let body = entry.get_body_decoded(content_index as u8);
            if body.sensor != sensor_id{
                break;
            }
            else
            {
                // Create plot data entry
                let blob_plot_data = BlobPlotData{
                    device: entry.get_field_value(content_index as u8, StorageEntryFields::ConnectionDeviceId)
                        .as_str().unwrap().to_string(),
                    sensor: body.sensor,
                    date_time: entry.get_field_value(content_index as u8, StorageEntryFields::EnqueuedTime)
                        .as_str().unwrap().to_string(),
                    value: body.value
                };
                plot_data.push(blob_plot_data);
            }
        }
    }
    let mut json_result_string = serde_json::to_string(&plot_data);
    return json_result_string.unwrap();
}

async fn fetch_device_data(blobs: Vec<Blob>, mut client: StorageReadClient) -> Vec<Value> {
    let mut data_vec = vec![Value::Null; blobs.len()];
    for (index, mut blob) in blobs.iter().enumerate() {
        let data = client.get_blob_data(blob.name.to_owned()).await;
        println!("Data: {:?}", data);
        data_vec[index] = data;
    }
    data_vec
}

async fn get_blob_urls() -> (Vec<Blob>, StorageReadClient) {
    let mut client = azureblobmanager::storage::StorageReadClient::new(
        "messagesevents".to_string(),
        "".to_string(),
        "devicedata".to_string()
    );
    let blobs = client.get_list_blobs().await;
    return (blobs, client);
}

async fn get_all_blobs_day(date: chrono::DateTime<Utc>, blobs: Vec<Blob>) -> (Vec<Blob>){
    let result_blobs = StorageReadClient
    ::collect_all_blobs_day(blobs, date);
    let data = result_blobs.await;
    return data;
}


#[get("/status")]
async fn service_status() -> Template{
    let mut context: HashMap<&str, bool> = HashMap::new();
    let status = get_all_status();
    for state in status{
        context.insert(state.0, match state.1{ 200 => true, 400 => false, _ => false });
    }
    let mut final_hashmap: HashMap<&str, HashMap<&str, bool>> = HashMap::new();
    final_hashmap.insert("data", context);
    Template::render("status", &final_hashmap)
}

#[get("/images/<filename>")]
pub async fn images(filename: &str) -> (Status, Option<NamedFile>){
    // Get a image file
    let mut script = NamedFile::open(format!("images/{}", filename)).await;
    return if script.is_err() {
        // Failed to get the file.
        (Status::new(404), None)
    } else {
        (Status::new(200), Some(script.unwrap()))
    }
}

#[get("/scripts/<filename>")]
pub async fn scripts(filename: &str) -> (Status, Option<NamedFile>){
    // Get a script file
    let mut script = NamedFile::open(format!("scripts/{}", filename)).await;
    return if script.is_err() {
        // Failed to get the file.
        (Status::new(404), None)
    } else {
        (Status::new(200), Some(script.unwrap()))
    }
}

// All routes
#[get("/actions")]
pub fn actions() -> Template{
    let mut context: HashMap<&str, u8> = HashMap::new();
    Template::render("actions", &context)
}

#[get("/raspberrypi1")]
pub fn raspberrypi_airquality() -> Template {
    let mut context: HashMap<&str, bool> = HashMap::new();
    let ping_response = get_all_status();
    let state = ping_response["Airquality"];
    context.insert("state", match state { 200 => true, 400 => false, _ => false});
    Template::render("raspberrypi1", &context)
}

#[get("/raspberrypi2")]
pub fn raspberrypi_temperature() -> Template {
    let mut context: HashMap<&str, bool> = HashMap::new();
    let ping_response = get_all_status();
    let state = ping_response["Temperature"];
    context.insert("state", match state { 200 => true, 400 => false, _ => false});
    Template::render("raspberrypi2", &context)
}

#[get("/")]
pub fn index() -> Template{
    let mut context: HashMap<&str, u8> = HashMap::new();
    Template::render("index", &context)
}


#[get("/ping_all")]
pub fn ping_all() -> String {
    // Ping all set addresses.
    let ping_results = get_all_status();
    let json = serde_json::to_string(&ping_results);
    return json.unwrap();
}

pub fn ping_one(address: IpAddr) -> i32 {
    let timeout =  Duration::from_secs(3);
    let result = ping::ping(address, Some(timeout),
                            Some(166), Some(3), Some(1),
                            Some(&rand::random()));
    return if result.is_err() {
        // Failed to  ping
        404
    } else {
        200
    }
}

pub fn get_all_status() -> HashMap<&'static str, i32> {
    // Get all the service status devices
    let mut devices = HashMap::<&str,IpAddr>::new();
    devices.insert("Airquality", "192.168.1.44".parse().unwrap());
    devices.insert("Temperature", "192.168.1.42".parse().unwrap());
    let mut results = HashMap::<&str,i32>::new();
    for mut device in devices{
        let state = ping_one(device.1);
        results.insert(device.0, state);
    }
    return results;
}


#[rocket::main]
async fn main(){
    println!("Rocket is launching...");

    let rocket = rocket::build()
        .mount("/",
               routes![
                   devices,
                   actions,
                   raspberrypi_airquality,
                   raspberrypi_temperature,
                   index,
                   scripts,
                   images,
                   service_status,
                   ping_all,
                   device_data,
                   device_data_period,
                   get_available_dates
               ]
        )
        .attach(Template::fairing());

    let launch = rocket.launch().await;
    println!("Rocket end/crash: {:?}", launch);

}

#[ntex::main]
async fn ntex_main(message: TransferBody, target_device: String){
    println!("Main is launching...");
    // Device Params
    let mut service_key  = "";
    let mut hub_name = "";

    // TEST SER
    let mut sas_token = SasToken::service_token(
        service_key, 1, "researchprojecthub", "iothubowner"
    );

    let mut sas_token_opt = match sas_token{
        Ok(ok) => {
            Some(ok)
        }
        Err(fail) => {
            panic!("Failed SAS: {}",fail);
            None
        }
    };
    let token = sas_token_opt.unwrap();

    let mut client = amqp::client::ServiceClient::new(
        "", "src/root.pem",
        service_key, &token.sas, "iothubowner"
    ).await;

    let connector = client.connect().await;
    if connector.is_err(){
        // Failure
        return;
    }
    let action = Action{
        action: String::from("action")
    };
    let json = serde_json::to_string(&action);
    if json.is_err(){
        println!("Failed to send message");
        return;
    }
    else{
        client.send_simple_message(message, &target_device, 6 ).await;
    }
}



