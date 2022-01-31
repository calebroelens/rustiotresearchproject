pub use azure_storage_blobs::blob::Blob;

pub mod storage {
    use std::fmt::{Debug, Display, Error, Formatter};
    use std::future::Future;
    use std::io::Bytes;
    use std::str::FromStr;
    use std::string::FromUtf8Error;
    use std::sync::Arc;
    use azure_core::new_http_client;
    use azure_core::parsing::FromStringOptional;
    use azure_core::prelude::Range;
    use azure_storage::prelude::{StorageAccountClient, StorageClient};
    use azure_storage_blobs::blob::Blob;
    use azure_storage_blobs::blob::responses::GetBlobResponse;
    use azure_storage_blobs::prelude::{AsBlobClient, AsBlobServiceClient, AsContainerClient,
                                       BlobClient, BlobServiceClient, ContainerClient};
    use chrono::{Date, Datelike, DateTime, NaiveDateTime, ParseError, Utc};
    use serde_json::{from_str, Value};
    use serde::{Serialize, Deserialize};
    use crate::storage;
    use base64;
    use serde::de::value::BoolDeserializer;

    #[derive(Serialize, Deserialize)]
    pub struct BlobPlotData{
        pub device: String,
        pub sensor: String,
        pub date_time: String,
        pub value: f64
    }

    #[derive(Serialize, Deserialize)]
    pub struct Body{
        // Body struct
        pub sensor: String,
        pub value: f64
    }

    pub struct StorageEntry{
        blob_inner: Value,
    }
    impl StorageEntry{
        pub fn new(value: Value) -> StorageEntry {
            StorageEntry{
                blob_inner: value,
            }
        }
        pub fn get_field_value(&self, index: u8, field: StorageEntryFields) -> &Value {
            return match field {
                StorageEntryFields::ConnectionDeviceId => {
                    &self.blob_inner[index as usize]["SystemProperties"]["connectionDeviceId"]
                }
                StorageEntryFields::ConnectionAuthMethod => {
                    &self.blob_inner[index as usize]["SystemProperties"]["connectionAuthMethod"]
                }
                StorageEntryFields::ConnectionDeviceGenerationId => {
                    &self.blob_inner[index as usize]["SystemProperties"]["connectionDeviceGenerationId"]
                }
                StorageEntryFields::EnqueuedTime => {
                    &self.blob_inner[index as usize]["SystemProperties"]["enqueuedTime"]
                }
                StorageEntryFields::Body => {
                    &self.blob_inner[index as usize]["Body"]
                }
            }
        }
        pub fn get_body_decoded(&self, index: u8) -> Body {
            let body = self.get_field_value(index, StorageEntryFields::Body);
            let raw_body = body.as_str().unwrap();
            let raw_bytes = base64::decode(raw_body).unwrap();
            let string_data = String::from_utf8(raw_bytes).unwrap();
            let json: Body = serde_json::from_str(&string_data).unwrap();
            return json
        }

        pub fn parse_date_time(&self, index: u8) -> DateTime<Utc> {
            let date = self.get_field_value(index, StorageEntryFields::EnqueuedTime);
            let date_string = date.as_str().unwrap();
            DateTime::from(DateTime::parse_from_rfc3339(&date_string).unwrap())

        }
        pub fn get_data_as_vec(&self) -> &Vec<Value> {
            self.blob_inner.as_array().unwrap()
        }
        pub fn total_entries(&self) -> u32 {
            self.blob_inner.as_array().unwrap().len() as u32
        }

        pub fn create_blob_plot_data(&self, index: u8) -> BlobPlotData{
            let body = self.get_body_decoded(index);
            let sensor = body.sensor;
            let value = body.value;
            let device_id = self.get_field_value(index,
                                                 StorageEntryFields::ConnectionDeviceId)
                .as_str().unwrap();

            let date_time = self.get_field_value(index, StorageEntryFields::EnqueuedTime)
                .as_str().unwrap();

            BlobPlotData{
                device: device_id.to_string(),
                sensor,
                value,
                date_time: date_time.to_string()
            }
        }
    }

    pub enum StorageEntryFields{
        ConnectionDeviceId,
        ConnectionAuthMethod,
        ConnectionDeviceGenerationId,
        EnqueuedTime,
        Body,
    }

    pub struct StorageReadClient {
        storage_account: String,
        storage_key: String,
        storage_container: String,
        storage_client: Arc<StorageAccountClient>
    }
    impl StorageReadClient {
        pub fn new(storage_account: String, storage_key: String, storage_container: String) -> StorageReadClient{
            let http_client = new_http_client();
            let mut storage_client = StorageAccountClient
            ::new_access_key(http_client.clone(), &storage_account, &storage_key);
            StorageReadClient{
                storage_account,
                storage_key,
                storage_container,
                storage_client
            }
        }
        pub fn new_from_env() -> StorageReadClient {
            let http_client = new_http_client();
            let mut storage_client = StorageAccountClient
            ::new_access_key(http_client.clone(),
                             std::env::var("STORAGE_ACCOUNT").expect("Failed"),
                             std::env::var("STORAGE_KEY").expect("Failed"));
            StorageReadClient{
                storage_account: std::env::var("STORAGE_ACCOUNT").expect("Failed"),
                storage_key: std::env::var("STORAGE_KEY").expect("Failed") ,
                storage_container: std::env::var("STORAGE_CONTAINER").expect("Failed"),
                storage_client
            }
        }
        pub fn get_blob_service_client(&mut self) -> Arc<BlobServiceClient> {
            self.storage_client.as_blob_service_client()
        }
        pub fn get_container_client(&mut self) -> Arc<ContainerClient> {
            self.storage_client.as_container_client(&self.storage_container)
        }
        pub fn get_account_storage_client(&mut self) -> &Arc<StorageAccountClient> {
            &self.storage_client
        }
        pub async fn get_list_blobs(&mut self) -> Vec<Blob> {
            let container_client = self.storage_client.as_container_client(&self.storage_container);
            let blobs = container_client.list_blobs().execute().await.unwrap();
            blobs.blobs.blobs
        }

        pub async fn collect_all_blobs_inner_value(&mut self) -> Vec<Value> {
            let blob_list = self.get_list_blobs().await;
            let mut inner_values_vec = Vec::<Value>::new();
            for blob in blob_list{
                let blob_data = self.get_blob_data(blob.name).await;
                inner_values_vec.push(blob_data.clone());
            }
            return inner_values_vec;
        }

        pub async fn collect_all_blobs_day(all_blobs: Vec<Blob>, datetime: DateTime<Utc>) -> Vec<Blob> {
            // Collect all blobs inner data from a certain day.
            let mut selected_blobs = Vec::<Blob>::new();
            for blob in all_blobs{
                if blob.properties.last_modified.date() == datetime.date(){
                    // If the same data --> Push
                    selected_blobs.push(blob.clone());
                }
            }
            return selected_blobs;
        }

        pub async fn get_vec_of_blobs(&mut self, blobs: Vec<Blob>) -> Vec<Value> {
            // Gets all data of all entries
            let mut blob_received = Vec::<Value>::new();
            // EIterate over every blob  --> Execute --> Get their data
            let mut blob_client = self.storage_client.as_container_client(&self.storage_container);
            for(mut index, mut blob) in blobs.iter().enumerate(){
                let local_client = blob_client.as_blob_client(&blob.name);
                let local_blob_data = local_client
                    .get()
                    .range(Range::new(0,256000))
                    .execute()
                    .await;
                // Convert the data to a normal data format.
                let normalized_data = self.normalize_blob_data(
                    local_blob_data.unwrap()).await;
                let prepare_json_as_str = &*format!(
                    "[{}]",
                    String::from_utf8(normalized_data)
                        .unwrap());
                let as_json: Value = serde_json::from_str(prepare_json_as_str).unwrap();
                let entries = as_json.as_array().unwrap();
                for entry in entries{
                    blob_received.push(entry.clone());
                }
            }
            return blob_received;

        }

        pub async fn normalize_blob_data(&mut self, blob_response: GetBlobResponse) -> Vec<u8> {
            // Remove illegal JSON chars
            let mut unpacked_inner_vec_data = blob_response.data.to_vec();
            let mut result_data = String::new();
            let mut clone_data = unpacked_inner_vec_data.clone();
            for(mut pos, mut data_entry) in unpacked_inner_vec_data.iter().enumerate(){
                let one_c_string = String::from_utf8(vec![*data_entry;1]);
                match one_c_string{
                    Ok(ok_char) => {
                        let matcher = &*ok_char;
                        match matcher{
                            "\r" => {
                                // Filter this entry.
                                // Windows based CRLF
                                clone_data[pos] = ",".to_string().as_bytes().to_owned()[0];
                                clone_data[pos+1] = " ".to_string().as_bytes().to_owned()[0];
                            }
                            _ => {
                                // Ignore this entry.
                            }
                        }
                    }
                    Err(err_char) => {
                        println!("Exception while reading blob contents: {:?}", err_char);
                    }
                }
            }
            // End of convert
            // Return
            return clone_data;
        }

        pub async fn get_blob_data(&mut self, url: String) -> Value {
            let blob_client = self.storage_client.as_container_client(&self.storage_container).as_blob_client(&url);
            let response = blob_client
                .get()
                .range(Range::new(0,256000))
                .execute()
                .await;
            let data = response.unwrap();
            let data_copy = self.normalize_blob_data(data).await;
            // Data convert complete
            // Attempt to convert to json serde data object (Value)
            let convert_to_string: &str = &*format!("[{}]",String::from_utf8(data_copy).unwrap());
            let structured_data: Value = serde_json::from_str(convert_to_string).unwrap();
            return structured_data
        }
    }
    impl Display for StorageReadClient{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            let content = format!("StorageAccount: {}\n Storage container: {}",
                                  self.storage_account,self.storage_container);
            write!(f, "{}", content)
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::storage;
    use crate::storage::{StorageEntry, StorageEntryFields};

    #[tokio::test]
    async fn testing_azure_blob_list(){
        let as_acc = "messagesevents";
        let as_container = "devicedata";
        let as_key = "";
        let mut client = storage::StorageReadClient::new(as_acc.to_string(),
                                                         as_key.to_string(),
                                                         as_container.to_string());
        let blobs = client.get_list_blobs().await;
        for blob in blobs {
            println!("Blob: {}", blob.name);
        }
    }

    #[tokio::test]
    async fn testing_azure_stream(){
        let as_acc = "messagesevents";
        let as_container = "devicedata";
        let as_key = "";
        let mut client = storage::StorageReadClient::new(as_acc.to_string(),
                                                         as_key.to_string(),
                                                         as_container.to_string());
        let blobs = client.get_list_blobs().await;
        for blob in blobs {
            println!("Blob Stream start: {}", blob.name);
            let data = client.get_blob_data(blob.name).await;
            println!("Data: {}", data);
            let mut storage_blob = StorageEntry::new(data);
            println!("Total body in blob: {}", storage_blob.total_entries());
            let mut body = storage_blob.get_body_decoded(0);
            println!("Body:\nSensorID: {}\nValue: {}", body.sensor, body.value);
        }
    }
}
