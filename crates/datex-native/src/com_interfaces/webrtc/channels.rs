use std::sync::Arc;

use datex_core::{
    channel::mpsc,
    network::com_interfaces::default_setup_data::webrtc::{
        NativeWebRTCSignaling, RTCSdpTypeDX, WebRTCInterfaceSetupData,
        WebRTCSignalDX,
    },
};
use futures::channel::oneshot;
use log::{error, warn};
use webrtc::{
    data_channel::{
        RTCDataChannel, data_channel_message::DataChannelMessage,
        data_channel_state::RTCDataChannelState,
    },
    peer_connection::RTCPeerConnection,
};

use crate::com_interfaces::webrtc::mapping::*;

pub fn install_ice_callback(
    peer_connection: Arc<RTCPeerConnection>,
    signaling: Arc<dyn NativeWebRTCSignaling>,
) {
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        let signaling = signaling.clone();
        Box::pin(async move {
            let signal = match candidate {
                Some(candidate) => {
                    let json = match candidate.to_json() {
                        Ok(json) => json,
                        Err(e) => {
                            error!("Failed to convert ICE candidate: {e}");
                            return;
                        }
                    };
                    WebRTCSignalDX::IceCandidate(make_dx_candidate(json))
                }
                None => WebRTCSignalDX::EndOfCandidates,
            };

            if let Err(e) = signaling.send(signal).await {
                warn!("Failed to send ICE candidate: {e}");
            }
        })
    }));
}

pub async fn create_offerer_channel(
    setup: &WebRTCInterfaceSetupData,
    peer_connection: Arc<RTCPeerConnection>,
    signaling: Arc<dyn NativeWebRTCSignaling>,
    incoming_tx: mpsc::Sender<Vec<u8>>,
) -> Result<Arc<RTCDataChannel>, String> {
    let data_channel = peer_connection
        .create_data_channel(
            &setup.data_channel_label,
            Some(make_data_channel_init(setup)),
        )
        .await
        .map_err(|e| e.to_string())?;

    let open_rx =
        install_data_channel_callbacks(data_channel.clone(), incoming_tx);
    let offer = peer_connection
        .create_offer(None)
        .await
        .map_err(|e| e.to_string())?;
    peer_connection
        .set_local_description(offer.clone())
        .await
        .map_err(|e| e.to_string())?;

    signaling
        .send(WebRTCSignalDX::Description(make_dx_description(offer)))
        .await?;

    let mut got_answer = false;
    let mut open_rx = Box::pin(open_rx);
    loop {
        tokio::select! {
            open_result = &mut open_rx => {
                open_result.map_err(|_| "data channel closed before open".to_string())?;
                if got_answer {
                    break;
                }
            }

            signal = signaling.receive() => {
                match signal? {
                    WebRTCSignalDX::Description(description) => {
                        if description.sdp_type != RTCSdpTypeDX::Answer {
                            return Err("expected WebRTC answer".to_string());
                        }
                        let description = make_sdp_description(description)?;
                        peer_connection
                            .set_remote_description(description)
                            .await
                            .map_err(|e| e.to_string())?;
                        got_answer = true;
                    }

                    WebRTCSignalDX::IceCandidate(candidate) => {
                        peer_connection
                            .add_ice_candidate(make_ice_candidate(candidate))
                            .await
                            .map_err(|e| e.to_string())?;
                    }

                    WebRTCSignalDX::EndOfCandidates => {}
                }

                if got_answer && data_channel.ready_state() == RTCDataChannelState::Open {
                    break;
                }
            }
        }
    }

    Ok(data_channel)
}

pub async fn create_answerer_channel(
    peer_connection: Arc<RTCPeerConnection>,
    signaling: Arc<dyn NativeWebRTCSignaling>,
    incoming_tx: mpsc::Sender<Vec<u8>>,
) -> Result<Arc<RTCDataChannel>, String> {
    let (dc_tx, dc_rx) = oneshot::channel::<Arc<RTCDataChannel>>();
    let dc_tx = Arc::new(tokio::sync::Mutex::new(Some(dc_tx)));
    peer_connection.on_data_channel(Box::new(move |data_channel| {
        let dc_tx = dc_tx.clone();
        Box::pin(async move {
            if let Some(tx) = dc_tx.lock().await.take() {
                let _ = tx.send(data_channel);
            }
        })
    }));

    loop {
        match signaling.receive().await? {
            WebRTCSignalDX::Description(description) => {
                if description.sdp_type != RTCSdpTypeDX::Offer {
                    return Err("expected WebRTC offer".to_string());
                }
                let description = make_sdp_description(description)?;
                peer_connection
                    .set_remote_description(description)
                    .await
                    .map_err(|e| e.to_string())?;
                let answer = peer_connection
                    .create_answer(None)
                    .await
                    .map_err(|e| e.to_string())?;
                peer_connection
                    .set_local_description(answer.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                signaling
                    .send(WebRTCSignalDX::Description(make_dx_description(
                        answer,
                    )))
                    .await?;

                break;
            }

            WebRTCSignalDX::IceCandidate(candidate) => {
                peer_connection
                    .add_ice_candidate(make_ice_candidate(candidate))
                    .await
                    .map_err(|e| e.to_string())?;
            }

            WebRTCSignalDX::EndOfCandidates => {}
        }
    }
    let data_channel = dc_rx
        .await
        .map_err(|_| "remote did not create data channel".to_string())?;
    let open_rx =
        install_data_channel_callbacks(data_channel.clone(), incoming_tx);
    open_rx
        .await
        .map_err(|_| "data channel closed before open".to_string())?;
    Ok(data_channel)
}

pub fn install_data_channel_callbacks(
    data_channel: Arc<RTCDataChannel>,
    incoming_tx: mpsc::Sender<Vec<u8>>,
) -> oneshot::Receiver<()> {
    let (open_tx, open_rx) = oneshot::channel::<()>();
    data_channel.on_open(Box::new(move || {
        Box::pin(async move {
            let _ = open_tx.send(());
        })
    }));
    data_channel.on_message(Box::new(move |message: DataChannelMessage| {
        let mut incoming_tx = incoming_tx.clone();
        Box::pin(async move {
            let bytes = message.data.to_vec();
            if incoming_tx.send(bytes).await.is_err() {
                warn!("WebRTC DATEX receiver dropped");
            }
        })
    }));
    open_rx
}
