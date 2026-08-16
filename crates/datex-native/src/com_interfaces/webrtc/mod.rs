use std::{ops::Deref, sync::Arc};

use bytes::Bytes;
use datex_core::{
    channel::mpsc,
    macros::Datex,
    network::{
        com_hub::errors::ComInterfaceCreateError,
        com_interfaces::{
            com_interface::{
                factory::{
                    ComInterfaceAsyncFactory, ComInterfaceAsyncFactoryResult,
                    ComInterfaceConfiguration, SendCallback, SendFailure,
                    SocketConfiguration, SocketProperties,
                },
                properties::{ComInterfaceProperties, InterfaceDirection},
            },
            default_setup_data::webrtc::{
                WebRTCInterfaceSetupData, WebRTCRoleDX, WebRTCSignaling,
            },
        },
    },
};
mod channels;
use channels::*;
mod mapping;
use log::{error, warn};
use mapping::*;

use webrtc::{
    self, api::APIBuilder, peer_connection::configuration::RTCConfiguration,
};

#[derive(Datex)]
#[datex(structural_recursive)]
pub struct WebRTCInterfaceSetupDataNative {
    pub setup: WebRTCInterfaceSetupData,
    #[datex(skip)]
    pub signaling: Option<Arc<dyn WebRTCSignaling>>,
}

impl Deref for WebRTCInterfaceSetupDataNative {
    type Target = WebRTCInterfaceSetupData;
    fn deref(&self) -> &Self::Target {
        &self.setup
    }
}

impl WebRTCInterfaceSetupDataNative {
    pub fn new(
        setup: WebRTCInterfaceSetupData,
        signaling: Arc<dyn WebRTCSignaling>,
    ) -> Self {
        Self {
            setup,
            signaling: Some(signaling),
        }
    }

    async fn create_interface(
        self,
    ) -> Result<ComInterfaceConfiguration, ComInterfaceCreateError> {
        create_webrtc_interface(
            self.setup,
            self.signaling.expect("signaling must be provided"),
        )
        .await
        .map_err(ComInterfaceCreateError::connection_error_with_details)
    }
}

impl ComInterfaceAsyncFactory for WebRTCInterfaceSetupDataNative {
    fn create_interface(self) -> ComInterfaceAsyncFactoryResult {
        Box::pin(self.create_interface())
    }

    fn get_default_properties() -> ComInterfaceProperties {
        WebRTCInterfaceSetupData::get_default_properties()
    }
}

async fn create_webrtc_interface(
    setup: WebRTCInterfaceSetupData,
    signaling: Arc<dyn WebRTCSignaling>,
) -> Result<ComInterfaceConfiguration, String> {
    let api = APIBuilder::new().build();
    let peer_connection = Arc::new(
        api.new_peer_connection(RTCConfiguration {
            ice_servers: setup
                .ice_servers
                .clone()
                .into_iter()
                .map(make_ice_server)
                .collect(),
            ..RTCConfiguration::default()
        })
        .await
        .map_err(|e| e.to_string())?,
    );

    install_ice_callback(peer_connection.clone(), signaling.clone());
    let (incoming_tx, mut incoming_rx) =
        mpsc::create_bounded_channel::<Vec<u8>>(1024);

    let data_channel = match setup.role {
        WebRTCRoleDX::Offerer => {
            create_offerer_channel(
                &setup,
                peer_connection.clone(),
                signaling.clone(),
                incoming_tx,
            )
            .await?
        }
        WebRTCRoleDX::Answerer => {
            create_answerer_channel(
                peer_connection.clone(),
                signaling.clone(),
                incoming_tx,
            )
            .await?
        }
    };

    Ok(ComInterfaceConfiguration::new_single_socket(
        ComInterfaceProperties {
            name: Some(setup.data_channel_label),
            ..WebRTCInterfaceSetupData::get_default_properties()
        },
        SocketConfiguration::new_in_out(
            SocketProperties::new(InterfaceDirection::InOut, 1),
            async gen move {
                while let Some(bytes) = incoming_rx.next().await {
                    yield Ok(bytes);
                }
                warn!("WebRTC DataChannel closed");
            },
            SendCallback::new_async(move |block| {
                let data_channel = data_channel.clone();
                async move {
                    let bytes = Bytes::from(block.to_bytes());
                    data_channel.send(&bytes).await.map(|_| ()).map_err(|e| {
                        error!("WebRTC DataChannel send error: {e}");
                        SendFailure(Box::new(block))
                    })
                }
            }),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use datex_core::{
        channel::mpsc,
        network::com_interfaces::default_setup_data::webrtc::WebRTCSignalDX,
    };
    use std::{pin::Pin, time::Duration};
    use tokio::sync::Mutex;

    use super::*;
    use datex_core::{
        global::dxb_block::DXBBlock,
        network::com_interfaces::{
            com_interface::factory::SendSuccess,
            default_setup_data::webrtc::{
                WebRTCInterfaceSetupData, WebRTCRoleDX, WebRTCSignalResult,
            },
        },
        utils::async_iterators::async_next_pin_box,
    };

    pub struct MemoryWebRTCSignaling {
        tx: mpsc::Sender<WebRTCSignalDX>,
        rx: Arc<Mutex<mpsc::Receiver<WebRTCSignalDX>>>,
    }

    impl MemoryWebRTCSignaling {
        pub fn pair(
            buffer: usize,
        ) -> (Arc<MemoryWebRTCSignaling>, Arc<MemoryWebRTCSignaling>) {
            let (a_tx, a_rx) =
                mpsc::create_bounded_channel::<WebRTCSignalDX>(buffer);
            let (b_tx, b_rx) =
                mpsc::create_bounded_channel::<WebRTCSignalDX>(buffer);
            (
                Arc::new(MemoryWebRTCSignaling {
                    tx: a_tx,
                    rx: Arc::new(Mutex::new(b_rx)),
                }),
                Arc::new(MemoryWebRTCSignaling {
                    tx: b_tx,
                    rx: Arc::new(Mutex::new(a_rx)),
                }),
            )
        }
    }

    impl WebRTCSignaling for MemoryWebRTCSignaling {
        fn send(
            &self,
            signal: WebRTCSignalDX,
        ) -> Pin<Box<dyn Future<Output = WebRTCSignalResult<()>> + Send>>
        {
            let mut tx = self.tx.clone();
            Box::pin(async move {
                tx.send(signal).await.map_err(|_| {
                    "test WebRTC signaling receiver dropped".to_string()
                })
            })
        }
        fn receive(
            &self,
        ) -> Pin<
            Box<dyn Future<Output = WebRTCSignalResult<WebRTCSignalDX>> + Send>,
        > {
            let rx = self.rx.clone();
            Box::pin(async move {
                let mut rx = rx.lock().await;
                rx.next().await.ok_or_else(|| {
                    "test WebRTC signaling sender# dropped".to_string()
                })
            })
        }
    }

    #[tokio::test]
    async fn test_constructs() {
        let (offerer_signaling, answerer_signaling) =
            MemoryWebRTCSignaling::pair(1000);

        let offerer_setup = WebRTCInterfaceSetupData {
            role: WebRTCRoleDX::Offerer,
            data_channel_label: "datex".to_string(),
            ..WebRTCInterfaceSetupData::default()
        };
        let answerer_setup = WebRTCInterfaceSetupData {
            role: WebRTCRoleDX::Answerer,
            data_channel_label: "datex".to_string(),
            ..WebRTCInterfaceSetupData::default()
        };

        let offerer = WebRTCInterfaceSetupDataNative::new(
            offerer_setup,
            offerer_signaling,
        );

        let answerer = WebRTCInterfaceSetupDataNative::new(
            answerer_setup,
            answerer_signaling,
        );
        let (offerer_result, answerer_result) = tokio::join!(
            offerer.create_interface(),
            answerer.create_interface(),
        );
        assert!(offerer_result.is_ok());
        assert!(answerer_result.is_ok());
    }

    async fn next_socket(
        interface: &mut ComInterfaceConfiguration,
    ) -> SocketConfiguration {
        tokio::time::timeout(
            Duration::from_secs(10),
            async_next_pin_box(&mut interface.new_sockets_iterator),
        )
        .await
        .expect("timed out waiting for WebRTC socket")
        .expect("WebRTC interface produced no socket")
        .expect("WebRTC socket creation failed")
    }

    async fn next_bytes(socket: &mut SocketConfiguration) -> Vec<u8> {
        let iterator = socket
            .iterator
            .as_mut()
            .expect("WebRTC socket has no receive iterator");

        tokio::time::timeout(
            Duration::from_secs(10),
            async_next_pin_box(iterator),
        )
        .await
        .expect("timed out waiting for WebRTC data")
        .expect("WebRTC receive iterator ended")
        .expect("WebRTC receive iterator yielded error")
    }

    async fn send_block(socket: &SocketConfiguration, block: DXBBlock) {
        let send_callback = socket
            .send_callback
            .as_ref()
            .expect("WebRTC socket has no send callback");
        match send_callback {
            SendCallback::Sync(callback) | SendCallback::SyncOnce(callback) => {
                match callback(block).expect("sync WebRTC send failed") {
                    SendSuccess::Sent => {}
                    _ => unreachable!(
                        "sync WebRTC send callback returned unexpected result"
                    ),
                }
            }
            SendCallback::Async(callback) => {
                callback
                    .call(block)
                    .await
                    .expect("async WebRTC send failed");
            }
        }
    }

    #[tokio::test]
    async fn data_exchange() {
        let (offerer_signaling, answerer_signaling) =
            MemoryWebRTCSignaling::pair(1000);
        let offerer_setup = WebRTCInterfaceSetupData {
            role: WebRTCRoleDX::Offerer,
            data_channel_label: "datex".to_string(),
            ..WebRTCInterfaceSetupData::default()
        };
        let answerer_setup = WebRTCInterfaceSetupData {
            role: WebRTCRoleDX::Answerer,
            data_channel_label: "datex".to_string(),
            ..WebRTCInterfaceSetupData::default()
        };

        let offerer = WebRTCInterfaceSetupDataNative::new(
            offerer_setup,
            offerer_signaling,
        );
        let answerer = WebRTCInterfaceSetupDataNative::new(
            answerer_setup,
            answerer_signaling,
        );

        let (offerer_result, answerer_result) = tokio::join!(
            offerer.create_interface(),
            answerer.create_interface(),
        );

        let mut offerer_interface =
            offerer_result.expect("offerer create_interface failed");
        let mut answerer_interface =
            answerer_result.expect("answerer create_interface failed");

        let mut offerer_socket = next_socket(&mut offerer_interface).await;
        let mut answerer_socket = next_socket(&mut answerer_interface).await;

        let offerer_to_answerer =
            DXBBlock::new_with_body(b"hello from offerer");
        let offerer_to_answerer_bytes = offerer_to_answerer.to_bytes();

        send_block(&offerer_socket, offerer_to_answerer).await;

        let received_on_answerer = next_bytes(&mut answerer_socket).await;
        assert_eq!(received_on_answerer, offerer_to_answerer_bytes);

        let answerer_to_offerer =
            DXBBlock::new_with_body(b"hello from answerer");
        let answerer_to_offerer_bytes = answerer_to_offerer.to_bytes();

        send_block(&answerer_socket, answerer_to_offerer).await;

        let received_on_offerer = next_bytes(&mut offerer_socket).await;
        assert_eq!(received_on_offerer, answerer_to_offerer_bytes);
    }
}
