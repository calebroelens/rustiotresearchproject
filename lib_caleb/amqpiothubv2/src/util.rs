pub mod token{
    // Everything to do with token creation in this module.
    use std::fmt::{Display, Formatter};
    use std::ops::Add;
    use chrono::{DateTime, Duration, Utc};
    use hmac::{Hmac, Mac, NewMac};
    use sha2::Sha256;

    // SasToken struct.
    pub struct SasToken{
        pub sig: String,
        pub sas: String,
    }

    // SasToken Display Implementation
    impl Display for SasToken{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "Sig: {}\nSas: {}", self.sig, self.sas)
        }
    }

    impl SasToken{
        pub fn new(primary_key: &str, days: i64, hub_name: &str, device_id: &str) -> Result<SasToken, SasTokenCreateException> {
            // Check code encode
            let check_token_decode = SasToken::is_key_decode_good(&primary_key);
            if check_token_decode == false{
                return Err(SasTokenCreateException::InvalidPrimaryTokenEncoding);
            }
            // Token encode correct --> Continue
            // Check token length
            let check_token_length = SasToken::is_key_length_good(&primary_key);
            if check_token_length == false{
                return Err(SasTokenCreateException::InvalidPrimaryTokenLength);
            }
            // Token length correct --> Continue
            // Create timestamps (Days is consumed!)
            let future_time = SasToken::create_future_date(days);
            let timestamp = future_time.timestamp();

            // Create hub url
            let hub_url = SasToken::create_hub_url(hub_name, device_id);

            // Create signature
            let to_sign = SasToken::create_to_sign(hub_url, timestamp);

            // Every check done --> Generating a key should be safe now.
            let unwrapped_key = base64::decode(&primary_key).unwrap();
            // Start generation
            let mut mac  = Hmac::<Sha256>::new_from_slice(&unwrapped_key).unwrap();
            // SIGN GEN
            mac.update(to_sign.as_bytes());
            let mac_result = mac.finalize();
            let signature = base64::encode(mac_result.into_bytes());
            let pairs = &vec![("sig", signature)];
            let token_result = serde_urlencoded::to_string(pairs).unwrap();
            // Build up token signature
            let sas = SasToken::format_password_token(hub_name, &token_result, timestamp, device_id);
            Ok(SasToken{
                sig: token_result.to_owned(),
                sas
            })
        }

        pub fn service_token(service_key: &str, days: i64, hub_name: &str, service: &str) -> Result<SasToken, SasTokenCreateException> {
            // Check code encode
            let check_token_decode = SasToken::is_key_decode_good(&service_key);
            if check_token_decode == false{
                return Err(SasTokenCreateException::InvalidPrimaryTokenEncoding);
            }
            // Token encode correct --> Continue
            // Check token length
            let check_token_length = SasToken::is_key_length_good(&service_key);
            if check_token_length == false{
                return Err(SasTokenCreateException::InvalidPrimaryTokenLength);
            }
            let hostname = format!("{}.azure-devices.net", hub_name);

            let future_time = SasToken::create_future_date(days);
            let timestamp = future_time.timestamp();

            let to_sign =  format!("{}.azure-devices.net\n{}", hub_name, timestamp);
            println!("Sign: {}", to_sign);

            let unwrapped_key = base64::decode(&service_key).unwrap();
            // Start generation
            let mut mac  = Hmac::<Sha256>::new_from_slice(&unwrapped_key).unwrap();
            // SIGN GEN
            mac.update(to_sign.as_bytes());
            let mac_result = mac.finalize();
            let signature = base64::encode(mac_result.into_bytes());
            let pairs = &vec![("sig", signature)];
            let token_result = serde_urlencoded::to_string(pairs).unwrap();
            // Build up token signature
            let sas = SasToken::format_password_token_service(
                &token_result.clone(),
                timestamp,
                service,
                &hostname);
            Ok(SasToken{
                sig: token_result.to_owned(),
                sas
            })

        }

        // Static functions for SasToken
        pub fn is_key_length_good(key: &str) -> bool{
            let b64_key = base64::decode(&key);
            let check_hmac =
                Hmac::<Sha256>::new_from_slice(&b64_key.unwrap());
            !check_hmac.is_err()
        }

        pub fn is_key_decode_good(key: &str) -> bool {
            let b64_key = base64::decode(&key);
            if b64_key.is_err(){
                // Failed to decode the key
                return false
            }
            return true
        }

        pub fn hostname_from_iothub_name(name: String) -> String{
            format!("{}.azure-devices.net", name)
        }
        pub fn format_password_token(hub_name: &str,
                                     token_result: &String,
                                     expiry_timestamp: i64,
                                     device_id: &str) -> String{
            format!(
                "SharedAccessSignature sr={}.azure-devices.net%2Fdevices%2F{}&{}&se={}&skn={}",
                hub_name,
                device_id,
                token_result,
                expiry_timestamp,
                device_id
            )
        }
        pub fn format_password_token_service(
                                             token_result: &String,
                                             expiry_timestamp: i64,
                                             policy_name: &str,
                                            encoded_url: &str) -> String{
            /*
            format!(
                "SharedAccessSignature {}&se={}&skn={}&sr={}",
                token_result,
                expiry_timestamp,
                policy_name,
                encoded_url
            )
             */
            format!(
                "SharedAccessSignature sr={}&{}&se={}&skn={}",
                encoded_url,
                token_result,
                expiry_timestamp,
                policy_name

            )
        }
        pub fn create_hub_url(hub_name: &str, device_id: &str) -> String{
            format!("{}.azure-devices.net%2Fdevices%2F{}", hub_name, device_id)
        }
        pub fn create_future_date(days_in_future: i64) -> DateTime<Utc> {
            let mut time_now = chrono::offset::Utc::now();
            let mut add_days = Duration::days(days_in_future);
            time_now.add(add_days);
            time_now
        }
        pub fn create_to_sign(hub_url: String, expiry_timestamp: i64) -> String{
            format!("{}\n{}", hub_url, expiry_timestamp)
        }
    }

    // SasToken Exceptions on creation.
    pub enum SasTokenCreateException{
        Failed,
        InvalidPrimaryTokenEncoding,
        InvalidPrimaryTokenLength,
    }

    // SasToken Exceptions Display implementation
    impl Display for SasTokenCreateException{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match *self{
                SasTokenCreateException::Failed => write!(f, "{}", "Failed"),
                SasTokenCreateException::InvalidPrimaryTokenEncoding => write!(f, "{}", "InvalidPrimaryTokenEncoding"),
                SasTokenCreateException::InvalidPrimaryTokenLength => write!(f, "{}", "InvalidPrimaryTokenLength"),
            }
        }
    }

}