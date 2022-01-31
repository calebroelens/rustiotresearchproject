pub mod client{
    use std::fmt::{Display, Formatter};
    use std::result::Result::{Err, Ok};
    use std::sync::Arc;
    use std::time::Duration;
    use ntex_amqp::client::{ConnectError, SaslAuth};
    use ntex_amqp::{Connection, ReceiverLink, SenderLink, Session};
    use rustls::ClientConfig;
    use async_std::future;
    use async_std::future::TimeoutError;
    use async_std::prelude::*;
    use ntex::connect::rustls::RustlsConnector;
    use ntex::util::ByteString;
    use ntex_amqp::codec::protocol::{Handle, Transfer, TransferBody};
    use ntex_amqp::error::{AmqpProtocolError, DispatcherError};
    use tokio::task::JoinHandle;
    use crate::amqp::client::AmqpFailure::AlreadyActive;
    use crate::amqp::client::ClientRedirectRecovery::{AMQPProtocolFailure, Disconnected, NoThreadAvailable, ServiceDisconnect, ThreadJoinError, Timeout};
    use crate::amqp::config::{create_address, create_hostname, create_sas_login, create_tls_config, create_username, read_certificate, TlsConfigFailure};
    use crate::amqp::transfer::TransferExceptions;
    use crate::amqp::transfer::TransferExceptions::{LinkAlreadyActive, LinkAmqpProtocolError, LinkDetachedOrDoesNotExist, MessageAmqpProtocolError, MessageTimeOut, NoSession};
    use crate::util::token::{SasToken, SasTokenCreateException};

    pub struct Client{
        address: String,
        auth: SaslAuth,
        tls_config: ClientConfig,
        session: Option<Session>,
        hostname: String,
        device_id: String,
        hub_name: String,
        primary_key: String,
        spawner: Option<JoinHandle<Result<(), DispatcherError>>>,
        stream: Option<Connection>,
        recover_links: Option<Vec<(String,String)>>,
        pub recv_handles: Option<Vec<Handle>>,
        recv_recover_links: Option<Vec<(String, String)>>
    }

    pub struct ServiceClient{
        address: String,
        auth: SaslAuth,
        tls_config: ClientConfig,
        session: Option<Session>,
        hostname: String,
        device_id: String,
        hub_name: String,
        primary_key: String,
        spawner: Option<JoinHandle<Result<(), DispatcherError>>>,
        stream: Option<Connection>,
        recover_links: Option<Vec<(String,String)>>,
    }
    impl ServiceClient{
        // Variant of client.
        pub async fn new(hub_name: &str, cert_location: &str, primairy_key: &str, sas_token: &str, policy: &str) -> ServiceClient {
            // Create a service client
            let certificate = read_certificate(cert_location).await;
            if certificate.is_none(){
                panic!("Failed to find the certificate. End of program.");
            }
            let tls_config_create = create_tls_config(certificate.unwrap());
            let tls_config_opt = match tls_config_create{
                Ok(config) => {
                    Some(config)
                }
                Err(exception) => {
                    panic!("Failed to create the TLS config: {}", exception);
                    None

                }
            };
            let tls_config = tls_config_opt.unwrap();
            let username = &*format!("{}@sas.root.{}", policy, hub_name);
            let credentials = create_sas_login(&username, &sas_token);
            let hostname =  create_hostname(&hub_name);
            let address = create_address(&hub_name);

            ServiceClient{
                address,
                auth: credentials,
                tls_config,
                session: None,
                hostname,
                device_id: "".to_string(),
                hub_name: hub_name.to_string(),
                primary_key: primairy_key.to_string(),
                spawner: None,
                stream: None,
                recover_links: None
            }

        }

        pub async fn connect(&mut self) -> Result<(), AmqpFailure> {
            if self.session.is_some() {
                // Connection already exists
                return Err(AlreadyActive);
            }
            let mut driver = ntex_amqp::client::Connector::new()
                .connector(RustlsConnector::new(Arc::new(self.tls_config.clone())));
            driver.hostname(&*self.hostname);
            let username = self.auth.authn_id.clone();
            let password_sasl = self.auth.password.clone();
            let address = self.address.clone();
            println!("Login:\n{}\n{}", username, password_sasl);
            let sasl_result = driver.connect_sasl(address.clone(), SaslAuth{
                authz_id: ByteString::from_static(""),
                authn_id: username,
                password: password_sasl,
            }).await;
            if sasl_result.is_err(){
                // Failed to connect to something
                match sasl_result{
                    Ok(_) => {}
                    Err(err) => {
                        println!("{:?}", err);
                    }
                }
                return Err(AmqpFailure::FailedSasl);
            }
            let ssl_connected_client = sasl_result.unwrap();
            let main_amqp_stream = ssl_connected_client.sink();
            let spawner = ntex::rt::spawn(ssl_connected_client.start_default());
            let mut session = main_amqp_stream.open_session().await.unwrap();
            self.session = Some(session);
            self.spawner = Some(spawner);
            self.stream = Some(main_amqp_stream);
            Ok(())
        }
        pub async fn send_simple_message(&mut self, message: TransferBody, device: &str, timeout: u64) -> Result<(), TransferExceptions>{
            // Send one simple message
            let timeout = Duration::from_secs(timeout);
            // Attach the sender
            let local_session = self.session.as_mut();
            if local_session.is_none(){
                return Err(NoSession);
            }
            // There is a session
            let mut local_session = local_session.unwrap();
            let mut sender = local_session.build_sender_link(
                "devicebound_link",
                "/messages/devicebound"
            ).max_message_size(65535);
            let task = future::timeout(
                timeout,
                async{
                    sender.open().await
                }
            );
            let task_result = task.await;
            if task_result.is_err(){
                return Err(TransferExceptions::GeneralTimeout);
            }
            let task_unwrap = task_result.unwrap();
            if task_unwrap.is_err(){
                return Err(TransferExceptions::LinkAmqpProtocolError);
            }
            let sender = task_unwrap.unwrap();
            let sender_task = future::timeout(
                timeout,
                async{
                    sender.send(message).await
                }
            ).await;

            if sender_task.is_err(){
                return Err(TransferExceptions::MessageTimeOut);
            }
            let sender_task_unwrap =  sender_task.unwrap();
            if sender_task_unwrap.is_err(){
                // Failed
                return Err(TransferExceptions::MessageAmqpProtocolError);
            }
            println!("Message disposition: {:?}", sender_task_unwrap.unwrap());
            Ok(())
        }
    }

    pub enum AmqpFailure
    {
        AlreadyActive,
        FailedSasl
    }
    impl Display for AmqpFailure{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match *self{
                AmqpFailure::AlreadyActive => write!(f, "AlreadyActive"),
                AmqpFailure::FailedSasl => write!(f, "FailedSasl")
            }
        }
    }

    pub enum ClientRedirectRecovery
    {
        GeneralFailure,
        NoThreadAvailable,
        Timeout,
        ThreadJoinError,
        ServiceDisconnect,
        AMQPCodecFailure,
        AMQPIOFailure,
        AMQPProtocolFailure,
        Disconnected,
    }

    impl Display for ClientRedirectRecovery{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match *self{
                ClientRedirectRecovery::GeneralFailure => write!(f, "GeneralFailure"),
                ClientRedirectRecovery::NoThreadAvailable => write!(f, "NoThreadAvailable"),
                ClientRedirectRecovery::Timeout => write!(f, "Timeout"),
                ClientRedirectRecovery::ThreadJoinError => write!(f, "ThreadJoinError"),
                ClientRedirectRecovery::ServiceDisconnect => write!(f, "ServiceDisconnect"),
                ClientRedirectRecovery::AMQPCodecFailure => write!(f, "AMQPCodedFailure"),
                ClientRedirectRecovery::AMQPIOFailure => write!(f, "AMQPIOFailure"),
                ClientRedirectRecovery::AMQPProtocolFailure => write!(f, "AMQPProtocolFailure"),
                ClientRedirectRecovery::Disconnected => write!(f, "Disconnected"),
            }
        }
    }


    impl Client{
        // Creates the client, but does not connect it yet
        pub async fn new(device_id: &str, hub_name: &str, cert_location: &str, primary_key: &str, sas_token: &str) -> Client {
            let certificate = read_certificate(cert_location).await;
            if certificate.is_none(){
                panic!("Failed to find the certificate. End of program.");
            }
            let tls_config_create = create_tls_config(certificate.unwrap());
            let tls_config_opt = match tls_config_create{
                Ok(config) => {
                    Some(config)
                }
                Err(exception) => {
                    panic!("Failed to create the TLS config: {}", exception);
                    None

                }
            };
            let tls_config = tls_config_opt.unwrap();
            // Create the other params:
            let username = create_username(&device_id, &hub_name);
            let credentials = create_sas_login(&username, &sas_token);
            let hostname = create_hostname(&hub_name);
            let address = create_address(&hub_name);

            Client{
                auth: credentials,
                tls_config,
                hostname: hostname.to_string(),
                device_id: device_id.to_string(),
                hub_name: hub_name.to_string(),
                primary_key: primary_key.to_string(),
                session: None,
                address,
                spawner: None,
                stream: None,
                recover_links: None,
                recv_handles: None,
                recv_recover_links: None
            }
        }

        pub async fn recover(&mut self) -> Result<(), ClientRedirectRecovery>{
            // When an error occurs, try to recover the program.
            println!("Attempt recovery");
            println!("Clearing drivers....");
            let mut disconnection = self.disconnect(5).await;
            if disconnection.is_err(){
                // Failed to disconnect
                panic!("Failed to disconnect: Server did not react!");
            }
            let mut disconnection_unwrap = disconnection.unwrap();
            if disconnection_unwrap.is_err(){
                panic!("Failed to disconnect: Server returned AMQP protocol error");
            }
            // Detach everything
            self.session = None;
            self.spawner = None;
            self.stream = None;

            let mut new_sas = match SasToken::new(
                &self.primary_key,
                1,
                &self.hub_name,
                &self.device_id) {
                Ok(token) => {
                    Some(token)
                }
                Err(_) => {
                    None
                }
            };

            // Generate new AUTH credentials.
            self.update_sas_token(&new_sas.unwrap().sas);
            println!("Attempting to reconnect...");
            let mut reconnect_result = self.connect().await;
            match reconnect_result{
                Ok(ok) => {
                    println!("Reconnected: Ok")
                }
                Err(err) => {
                    println!("Fail reconnect: {}", err);
                }
            }
            Ok(())
        }

        pub async fn attempt_get_runtime_exception(&mut self, timeout: u64) -> Result<(), ClientRedirectRecovery>{
            if self.spawner.is_none(){
                // Unsafe to abort
                return Err(NoThreadAvailable);
            }
            let mut local_spawner=  self.spawner.as_mut().unwrap();
            let timeout = Duration::from_secs(timeout);
            let exception_task = future::timeout(
                timeout, async{
                    local_spawner.await
                }
            ).await;
            if exception_task.is_err(){
                return Err(ClientRedirectRecovery::Timeout);
            }
            let unwrapped_task = exception_task.unwrap();
            if unwrapped_task.is_err(){
                println!("Test: Failed to join runtime and get exception state");
                return Err(ClientRedirectRecovery::ThreadJoinError);
            }
            let dispatcher_result = unwrapped_task.unwrap();
            return match dispatcher_result {
                Ok(_) => {
                    Ok(())
                }
                Err(error) => {
                    match error {
                        DispatcherError::Service => {
                            println!("Test: Server disconnected.");
                            Err(ClientRedirectRecovery::ServiceDisconnect)
                        }
                        DispatcherError::Codec(codec) => {
                            println!("Test: Codec error: {:?}", codec);
                            Err(ClientRedirectRecovery::AMQPCodecFailure)
                        }
                        DispatcherError::Protocol(proto) => {
                            println!("Test: Proto error: {:?}", proto);
                            Err(ClientRedirectRecovery::AMQPProtocolFailure)
                        }
                        DispatcherError::Disconnected => {
                            println!("Test: Disconnected");
                            Err(ClientRedirectRecovery::Disconnected)
                        }
                        DispatcherError::Io(io) => {
                            println!("Test: IO error: {:?}", io);
                            Err(ClientRedirectRecovery::AMQPIOFailure)
                        }
                    }
                }
            };
        }

        pub fn update_sas_token(&mut self, new_sas_token: &str){
            let mut username = create_username(&self.device_id, &self.hub_name);
            let mut credentials = create_sas_login(&username, new_sas_token);
            self.auth = credentials;
        }
        pub async fn disconnect(&mut self, max_timeout: i32) -> Result<Result<(), AmqpProtocolError>, TimeoutError> {
            if self.session.is_none(){
                // No session to unwrap!
                panic!("There is no session to unwrap");
            }
            let disconnect_result = future::timeout(
                Duration::from_secs(max_timeout as u64), async {
                    self.session.as_ref().unwrap().close().await
                }
            ).await;
            return disconnect_result;
        }

        pub async fn send_message(&mut self, sender_link_name: &str, message: TransferBody, timeout: u64) -> Result<(), TransferExceptions> {
            if self.session.is_none(){
                // No session available
                return Err(NoSession);
            }
            // Session available
            // Check for sender link existence
            let mut local_session = self.session.as_mut().unwrap();
            let mut sender_link_local = local_session.get_sender_link(sender_link_name);
            if sender_link_local.is_none(){
                return Err(LinkDetachedOrDoesNotExist);
            }
            // Link exists!
            // Attempt to send a message
            let timeout = Duration::from_secs(timeout);
            let timed_send = future::timeout(
                timeout, async {
                    sender_link_local.unwrap().send(message).await
                }
            ).await;
            if timed_send.is_err(){
                return Err(MessageTimeOut);
            }
            // No timeout
            let msg_result = timed_send.unwrap();
            if msg_result.is_err(){
                println!("Error on send: {:?}", msg_result);
                return Err(MessageAmqpProtocolError);
            }
            Ok(())
        }
        pub fn retrieve_receiver_link(&mut self, handle: Handle) -> Option<&ReceiverLink> {
            self.session.as_mut().unwrap().get_receiver_link_by_handle(handle)
        }


        pub async fn attach_receiver(&mut self, name: &str, address: &str, timeout: u64) -> Result<(), TransferExceptions>{
            // Create a receiver link.
            if self.session.is_none(){
                return Err(NoSession);
            }
            let mut local_session = self.session.as_mut().unwrap();

            // Dont check if the receiver already exists.
            if self.recv_handles.is_none(){
                // The handles are empty --> Start creation.
            }
            else {
                let handles = self.recv_handles.as_mut().unwrap();
                let mut handle_index = 65535;
                for (index, handle) in handles.iter().enumerate() {
                    if local_session.get_receiver_link_by_handle(handle.to_owned()).is_some() {
                        handle_index = index;
                        break;
                    }
                }
                if handle_index != 65535 {
                    // Good, no link already created with the same handle.
                    return Err(TransferExceptions::LinkAlreadyActive);
                }
            }

            // Create the handle
            let mut new_recv_link = local_session.build_receiver_link(
                name,
                address,
            ).max_message_size(65535);

            let timeout = Duration::from_secs(timeout);
            let create_task = future::timeout(
              timeout, async
                    {
                        new_recv_link.open().await
                    }
            ).await;
            if create_task.is_err(){
                // Task timed out
                return Err(TransferExceptions::GeneralTimeout);
            }
            let unwrap_create_task = create_task.unwrap();
            if unwrap_create_task.is_err(){
                // The request to create the recv link failed.
                return Err(TransferExceptions::LinkCreateFailure);
            }
            let mut get_link = unwrap_create_task.unwrap();
            get_link.set_link_credit(360);
            // Add the handle to handle vec.
            if self.recv_handles.is_none(){
                // Create the instance
                self.recv_handles = Some(vec![get_link.handle()])
            }
            else
            {
                // Local
                let mut local_handle_vec = self.recv_handles.as_mut().unwrap();
                local_handle_vec.push(get_link.handle());
            }
            // Add the properties of the handle to the array.
            if self.recv_recover_links.is_none()
            {
                self.recv_recover_links = Some(vec![(name.to_string(), address.to_string())]);
            }
            else
            {
                let local_handle_vec = self.recv_recover_links.as_mut().unwrap();
                local_handle_vec.push((name.to_string(), address.to_string()))
            }

            Ok(())
        }

        pub async fn attach_sender(&mut self, name: &str, address: &str, timeout: u64) -> Result<(), TransferExceptions>{
            if self.session.is_none(){
                return Err(NoSession);
            }
            let mut local_session = self.session.as_mut().unwrap();
            // Check if the link already exists
            let mut local_sender_link = local_session.get_sender_link(&name);
            if local_sender_link.is_some(){
                return Err(LinkAlreadyActive);
            }
            // Passed all checks --> Lets try creating the sender link
            let timeout = Duration::from_secs(timeout);
            let sender_link_build = local_session.build_sender_link(
                name,
                address
            ).max_message_size(65535);

            let create_link_task = future::timeout(
                timeout, async{
                    sender_link_build.open().await
                }
            ).await;

            if create_link_task.is_err(){
                // Failed -> Timeout
                return Err(TransferExceptions::GeneralTimeout)
            }
            // No error
            let create_result = create_link_task.unwrap();
            if create_result.is_err(){
                return Err(LinkAmqpProtocolError);
            }
            // Creation success.
            if self.recover_links.is_none(){
                self.recover_links = Some(vec![(name.to_string(), address.to_string())]);
            }
            else{
                let mut local_recover_links = self.recover_links.as_mut().unwrap();
                local_recover_links.push((name.to_string(), address.to_string()));
            }
            Ok(())
        }

        pub async fn reattach_sender_links(&mut self){
            // Reattach the sender link
            let mut links = self.recover_links.as_mut().cloned();
            if links.is_none(){
                // No links
                println!("No links to recover!");
            }
            else{
                // Links
                let mut reattach_links = links.unwrap();
                // Reset initial links.
                self.recover_links = None;
                for link in reattach_links{
                    self.attach_sender(
                        &*link.0,
                        &*link.1,
                        5
                    ).await;
                }
            }
        }

        pub async fn reattach_receiver_links(&mut self){
            // Reset the handles
            self.recv_handles = None;
            // Loop the recv if possible
            let mut links = self.recv_recover_links.as_mut().cloned();
            self.recv_recover_links = None;
            for link in links.unwrap(){
                self.attach_receiver(
                    &*link.0,
                    &*link.1,
                    5
                ).await;
            }

        }

        pub async fn receive_message_listener(&mut self, link_index: u32, msg_timeout: u64) -> Result<Transfer, TransferExceptions> {
            let recv_handles = self.recv_handles.as_ref();
            let handle= match recv_handles{
                None => {
                    // No handles applied
                    return Err(TransferExceptions::LinkDetachedOrDoesNotExist);
                }
                Some(recv_handles) => {
                    // Handles found
                    let mut handle = recv_handles.get(0);
                    if handle.is_none(){
                        return Err(TransferExceptions::LinkDetachedOrDoesNotExist);
                    }
                    else
                    {
                        handle.unwrap()
                    }
                }
            };

            let mut link = match self.retrieve_receiver_link(*handle)
            {
                None => {
                    return Err(TransferExceptions::LinkDetachedOrDoesNotExist);
                }
                Some(link) => {
                    link
                }
            };
            let mut timeout = Duration::from_secs(msg_timeout);
            let mut recv_task = future::timeout(
                timeout,
                async{
                    use futures::StreamExt;
                    let mut streams = futures::stream::select_all(vec![link.clone()]);
                    let item = futures::StreamExt::next(&mut streams).await;
                    return item;
                }
            ).await;
            return if recv_task.is_err() {
                Err(TransferExceptions::NoMessage)
            }
            else
            {
                let transfer_option = recv_task.unwrap();
                if transfer_option.is_some() {
                    match transfer_option.unwrap() {
                        Ok(transfer) => {
                            println!("Received RTransfer content. Reading...");
                            Ok(transfer)
                        }
                        Err(failure) => {
                            println!("Received an exception while attempting to get RTransfer.");
                            Err(TransferExceptions::LinkAmqpProtocolError)
                        }
                    }
                } else {
                    println!("Message contains nothing.");
                    Err(TransferExceptions::NoMessage)
                }
            }
        }


        pub async fn connect(&mut self) -> Result<(), AmqpFailure>{
            // Start the connection
            if self.session.is_some() {
                // Connection already exists
                return Err(AlreadyActive);
            }
            // Create connector
            let mut driver = ntex_amqp::client::Connector::new()
                .connector(RustlsConnector::new(Arc::new(self.tls_config.clone())));
            driver.hostname(&*self.hostname);
            let username = self.auth.authn_id.clone();
            let password_sasl = self.auth.password.clone();
            let address = self.address.clone();
            println!("Login:\n{}\n{}", username, password_sasl);
            let sasl_result = driver.connect_sasl(address.clone(), SaslAuth{
                authz_id: ByteString::from_static(""),
                authn_id: username,
                password: password_sasl,
            }).await;
            if sasl_result.is_err(){
                // Failed to connect to something
                match sasl_result{
                    Ok(_) => {}
                    Err(err) => {
                        println!("{:?}", err);
                    }
                }
                return Err(AmqpFailure::FailedSasl);
            }
            let ssl_connected_client = sasl_result.unwrap();
            let main_amqp_stream = ssl_connected_client.sink();
            let spawner = ntex::rt::spawn(ssl_connected_client.start_default());
            let mut session = main_amqp_stream.open_session().await.unwrap();
            self.session = Some(session);
            self.spawner = Some(spawner);
            self.stream = Some(main_amqp_stream);
            Ok(())
        }
    }
}

pub mod util{
    pub fn enable_logging_traces(trace: Option<&str>){
        // Allow for optional logging params
        if let Some(trace) = trace{
            std::env::set_var("RUST_LOG",trace);
        }
        else {
            std::env::set_var("RUST_LOG",
                              "ntex, ntex_amqp, std, async_std, rustls, urlencoding");
        }
        // Initiate the logger
        env_logger::init();
    }
}

pub mod transfer{
    use std::fmt::{Display, Formatter};
    use ntex::util::{Bytes, ByteString};
    use ntex_amqp::codec::protocol::{Address, Properties};

    // Create a transfer body from a str
    pub fn create_message_from_str(body: &str) -> ntex_amqp::codec::protocol::TransferBody{
        let content = ntex_amqp::codec::Message::with_body(Bytes::copy_from_slice(body.as_bytes()));
        ntex_amqp::codec::protocol::TransferBody::Message(Box::new(content))
    }
    // Create a transfer body from a String
    pub fn create_message_from_string(body: String) -> ntex_amqp::codec::protocol::TransferBody{
        let content = ntex_amqp::codec::Message::with_body(Bytes::copy_from_slice(body.as_bytes()));
        ntex_amqp::codec::protocol::TransferBody::Message(Box::new(content))
    }

    pub fn create_directed_message(body: String, target: String) -> ntex_amqp::codec::protocol::TransferBody{
        let mut content = ntex_amqp::codec::Message::with_body(Bytes::copy_from_slice(body.as_bytes()));
        let address = ByteString::from(target);
        let props = Properties{
            message_id: None,
            user_id: None,
            to: Some(address), // Set To prop to redirect to correct consumer queue.
            subject: None,
            reply_to: None,
            correlation_id: None,
            content_type: None,
            content_encoding: None,
            absolute_expiry_time: None,
            creation_time: None,
            group_id: None,
            group_sequence: None,
            reply_to_group_id: None
        };
        content.properties = Some(props);
        ntex_amqp::codec::protocol::TransferBody::Message(Box::new(content))
    }

    pub enum TransferExceptions{
        NoSession,
        LinkDetachedOrDoesNotExist,
        MessageTimeOut,
        MessageAmqpProtocolError,
        LinkAlreadyActive,
        GeneralTimeout,
        LinkAmqpProtocolError,
        LinkCreateFailure,
        NoMessage,
    }
    impl Display for TransferExceptions{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match *self{
                TransferExceptions::NoSession => write!(f, "NoSession"),
                TransferExceptions::LinkDetachedOrDoesNotExist => write!(f, "Link detached or non existent."),
                TransferExceptions::MessageTimeOut => write!(f, "Message sent timed out."),
                TransferExceptions::MessageAmqpProtocolError => write!(f, "Message encountered AMQP Exception."),
                TransferExceptions::LinkAlreadyActive => write!(f, "Link is already active"),
                TransferExceptions::GeneralTimeout => write!(f,"General timeout on operation."),
                TransferExceptions::LinkAmqpProtocolError => write!(f, "An AMQP error occurred on an link operation."),
                TransferExceptions::LinkCreateFailure => write!(f, "Failed to create the link."),
                TransferExceptions::NoMessage => write!(f, "No message"),
            }
        }
    }
}

pub mod config{
    use std::fmt::{Display, Formatter};
    // Required imports
    use std::io::Cursor;
    use std::path::Path;
    use ntex::util::ByteString;
    use ntex_amqp::client::SaslAuth;
    use rustls::ClientConfig;

    pub async fn read_certificate(certificate_path: &str) -> Option<Cursor<Vec<u8>>>{
        let path = Path::new(certificate_path);
        let mut certificate_reader = async_std::fs::read(path).await;
        if certificate_reader.is_err()
        {
            // Failed to read the certificate
            println!("Failed to read the certificate: Did not find the file!");
            return None
        }
        return Some(std::io::Cursor::new(certificate_reader.unwrap()));
    }

    pub fn create_sas_login(username: &str, password: &str) -> SaslAuth{
        SaslAuth{
            authz_id: ByteString::from_static(""),
            authn_id: username.into(),
            password: password.into()
        }
    }

    pub fn create_tls_config(mut certificate_vector: Cursor<Vec<u8>>) -> Result<ClientConfig, TlsConfigFailure> {
        let mut tls_config = ClientConfig::new();
        let mut tls_pem_add = tls_config.root_store.add_pem_file(&mut certificate_vector);
        if tls_pem_add.is_err(){
            // Failed to add to the root store of the config.
            return Err(TlsConfigFailure::CertAddToRootStoreFailure);
        }
        let tls_pem_result = tls_pem_add.unwrap();
        if tls_pem_result.0  == 0{
            // The cert  was invalid.
            return Err(TlsConfigFailure::NoValidCerts);
        }
        return Ok(tls_config);
    }
    pub enum TlsConfigFailure
    {
        CertAddToRootStoreFailure,
        NoValidCerts
    }
    impl Display for TlsConfigFailure{
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match *self{
                TlsConfigFailure::CertAddToRootStoreFailure => write!(f, "Failed to add the cert to the root store."),
                TlsConfigFailure::NoValidCerts => write!(f, "The applied certificate is invalid.")
            }
        }
    }
    pub fn create_username(device_id: &str, iot_hub_name: &str) -> String {
        format!("{}@sas.{}", &device_id, &iot_hub_name)
    }

    pub fn create_address(hub_name: &str) -> String{
        format!("{}.azure-devices.net:5671", hub_name)
    }
    pub fn create_hostname(hub_name: &str) -> String{
        format!("{}.azure-devices.net", hub_name)
    }

}