use core::result::Result;
use datex_core::{
    global::dxb_block::DXBBlock,
    macros::Datex,
    network::{
        com_hub::{
            errors::ComInterfaceCreateError,
            managers::com_interface_manager::ComInterfaceAsyncFactoryResult,
        },
        com_interfaces::{
            com_interface::{
                factory::{
                    ComInterfaceAsyncFactory, ComInterfaceConfiguration,
                    SendCallback, SendFailure, SendSuccess,
                    SocketConfiguration, SocketProperties,
                },
                properties::{ComInterfaceProperties, InterfaceDirection},
            },
            default_setup_data::serial::serial_client::SerialClientInterfaceSetupData,
        },
    },
};
use log::error;
use std::{
    ops::Deref,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::spawn_blocking;

#[derive(Datex)]
#[datex(structural)]
pub struct SerialClientInterfaceSetupDataNative(
    pub SerialClientInterfaceSetupData,
);
impl Deref for SerialClientInterfaceSetupDataNative {
    type Target = SerialClientInterfaceSetupData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SerialClientInterfaceSetupDataNative {
    const TIMEOUT: Duration = Duration::from_millis(1000);
    const BUFFER_SIZE: usize = 1024;

    async fn create_interface(
        self,
    ) -> Result<ComInterfaceConfiguration, ComInterfaceCreateError> {
        let port_name = self.port_name.clone().ok_or(
            ComInterfaceCreateError::invalid_setup_data(
                "Port name is required",
            ),
        )?;

        if port_name.is_empty() {
            return Err(ComInterfaceCreateError::InvalidSetupData(
                "Port name cannot be empty".to_string(),
            ));
        }

        let port_name_clone = port_name.clone();
        let port = spawn_blocking(move || {
            serialport::new(port_name_clone, self.baud_rate)
                .timeout(Self::TIMEOUT)
                .open()
        })
        .await
        .unwrap()
        .map_err(|err| {
            ComInterfaceCreateError::connection_error_with_details(err)
        })?;
        let port = Arc::new(Mutex::new(port));
        let port_clone = port.clone();

        Ok(ComInterfaceConfiguration::new_single_socket(
            ComInterfaceProperties {
                name: Some(port_name),
                ..Self::get_default_properties()
            },
            SocketConfiguration::new_in_out(
                SocketProperties::new(InterfaceDirection::InOut, 1),
                async gen move {
                    loop {
                        let result = spawn_blocking({
                            let port = port_clone.clone();
                            move || {
                                let mut buffer = [0u8; Self::BUFFER_SIZE];
                                match port.try_lock().unwrap().read(&mut buffer)
                                {
                                    Ok(n) if n > 0 => {
                                        Some(buffer[..n].to_vec())
                                    }
                                    _ => None,
                                }
                            }
                        })
                        .await;
                        match result {
                            Ok(Some(incoming)) => {
                                yield Ok(incoming);
                            }
                            _ => {
                                error!("Serial read error or shutdown");
                                return yield Err(());
                            }
                        }
                    }
                },
                SendCallback::new_sync(move |block: DXBBlock| {
                    port.lock()
                        .unwrap()
                        .write_all(block.to_bytes().as_slice())
                        .map_err(|e| {
                            error!("Serial write error: {e}");
                            SendFailure(Box::new(block))
                        })
                        .map(|_| SendSuccess::Sent)
                }),
            ),
        ))
    }
}

impl ComInterfaceAsyncFactory for SerialClientInterfaceSetupDataNative {
    fn create_interface(self) -> ComInterfaceAsyncFactoryResult {
        Box::pin(self.create_interface())
    }

    fn get_default_properties() -> ComInterfaceProperties {
        SerialClientInterfaceSetupData::get_default_properties()
    }
}
