//! This module contains helper functions for mapping between the datex_core WebRTC types and the webrtc crate types.
use datex_core::network::com_interfaces::default_setup_data::webrtc::{
    RTCIceCandidateInitDX, RTCIceServerDX, RTCSdpTypeDX,
    RTCSessionDescriptionDX, WebRTCInterfaceSetupData,
};
use webrtc::{
    data_channel::data_channel_init::RTCDataChannelInit,
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer,
    },
    peer_connection::sdp::{
        sdp_type::RTCSdpType, session_description::RTCSessionDescription,
    },
};

pub fn make_ice_server(server: RTCIceServerDX) -> RTCIceServer {
    RTCIceServer {
        urls: server.urls,
        username: server.username.unwrap_or_default(),
        credential: server.credential.unwrap_or_default(),
    }
}

pub fn make_ice_candidate(
    candidate: RTCIceCandidateInitDX,
) -> RTCIceCandidateInit {
    RTCIceCandidateInit {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_mline_index: candidate.sdp_mline_index,
        username_fragment: candidate.username_fragment,
    }
}

pub fn make_dx_candidate(
    candidate: RTCIceCandidateInit,
) -> RTCIceCandidateInitDX {
    RTCIceCandidateInitDX {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_mline_index: candidate.sdp_mline_index,
        username_fragment: candidate.username_fragment,
    }
}

pub fn make_sdp_description(
    description: RTCSessionDescriptionDX,
) -> Result<RTCSessionDescription, String> {
    match description.sdp_type {
        RTCSdpTypeDX::Offer => RTCSessionDescription::offer(description.sdp)
            .map_err(|e| e.to_string()),
        RTCSdpTypeDX::Answer => RTCSessionDescription::answer(description.sdp)
            .map_err(|e| e.to_string()),
        RTCSdpTypeDX::Unspecified => {
            Err("invalid WebRTC SDP type: unspecified".to_string())
        }
    }
}

pub fn make_dx_description(
    description: RTCSessionDescription,
) -> RTCSessionDescriptionDX {
    let sdp_type = match description.sdp_type {
        RTCSdpType::Offer => RTCSdpTypeDX::Offer,
        RTCSdpType::Answer => RTCSdpTypeDX::Answer,
        _ => RTCSdpTypeDX::Unspecified,
    };

    RTCSessionDescriptionDX {
        sdp_type,
        sdp: description.sdp,
    }
}

pub fn make_data_channel_init(
    setup: &WebRTCInterfaceSetupData,
) -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: Some(setup.ordered),
        negotiated: setup.negotiated_data_channel_id,
        ..RTCDataChannelInit::default()
    }
}
