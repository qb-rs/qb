use std::{sync::Arc, vec};

use qb::QBICommunication;
use qb_derive::QBIAsync;
use webrtc::{
    data_channel::data_channel_init::RTCDataChannelInit,
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{rtp_codec::RTPCodecType, RTCRtpTransceiverInit},
};

pub struct QBIRTCInit {
    pub peer_identity: String,
}

#[derive(QBIAsync)]
#[context(QBIRTCInit)]
pub struct QBIRTC {
    com: QBICommunication,
}

impl QBIRTC {
    async fn init_async(cx: QBIRTCInit, com: QBICommunication) -> Self {
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            peer_identity: cx.peer_identity,
            ..Default::default()
        };
        let api = webrtc::api::APIBuilder::new().build();
        let conn = api.new_peer_connection(config).await.unwrap();
        conn.add_transceiver_from_kind(RTPCodecType::Unspecified, None)
            .await
            .unwrap();

        conn.on_peer_connection_state_change(Box::new(|s| {
            println!("STATE CHANGE {:?}", s);
            Box::pin(async move {})
        }));
        let chan = conn
            .create_data_channel(
                "chan",
                Some(RTCDataChannelInit {
                    protocol: Some("QBP".to_string()),
                    negotiated: Some(1),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();

        conn.on_negotiation_needed(Box::new(|| {
            println!("NEGOTIATE");

            Box::pin(async move {})
        }));

        chan.on_message(Box::new(|msg| {
            println!(
                "Message from Peer: {}",
                String::from_utf8(msg.data.to_vec()).unwrap()
            );
            Box::pin(async move {})
        }));

        let chan_tx = Arc::clone(&chan);
        chan.on_open(Box::new(|| {
            Box::pin(async move {
                println!("ON OPEN");
                chan_tx.send_text("Hello World!").await.unwrap();
            })
        }));

        let answer = conn.create_offer(None).await.unwrap();

        // Create channel that is blocked until ICE Gathering is complete
        let mut gather_complete = conn.gathering_complete_promise().await;

        // Sets the LocalDescription, and starts our UDP listeners
        conn.set_local_description(answer).await.unwrap();

        // Block until ICE Gathering is complete, disabling trickle ICE
        // we do this because we only can exchange one signaling message
        // in a production application you should exchange ICE Candidates via OnICECandidate
        let _ = gather_complete.recv().await;

        println!("INIT SUCCESSFUL!");

        Self { com }
    }

    async fn run_async(mut self) {
        println!("{:?}", self.com.rx.recv().await.unwrap());
    }
}
