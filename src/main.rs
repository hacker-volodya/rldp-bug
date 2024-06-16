use std::{collections::HashSet, net::{Ipv4Addr, SocketAddrV4}, str::FromStr, time::Duration};

use anyhow::{Context, Result};

use bytes::Bytes;
use everscale_network::{
    adnl::{self, NodeIdFull, NodeIdShort},
    dht::NodeOptions,
    overlay::{self, OverlayOptions},
    proto::{self, overlay::NodeOwned},
    NetworkBuilder,
};
use rand::Rng;
use tl_proto::{TlRead, TlWrite};
use tracing_subscriber::fmt::format::FmtSpan;

mod global_config;

// NodeOwned { id: Ed25519 { key: [196, 13, 241, 117, 234, 97, 249, 180, 215, 62, 2, 9, 77, 7, 242, 125, 42, 102, 193, 205, 211, 206, 89, 194, 25, 205, 130, 103, 70, 45, 126, 25] }, overlay: [252, 6, 27, 161, 30, 29, 123, 169, 45, 198, 235, 37, 186, 121, 23, 74, 94, 164, 177, 30, 166, 41, 159, 156, 216, 13, 244, 33, 79, 29, 219, 59], version: 1718470331, signature: b"f\xb3\0\x7f\xfd\xe02`m\x87$'\xbe\xaa\xa25WD\xb0\xf9\x8a\xbb\x8d.|\xe0\xe7\x01*\xe9N\x8fB\xcc_\xb1*\x8d\x04|\xe3\x0c\xc6\xea&\n\xc2\x16\xa1\xf7M\xfc\x81\xf8\xaf\xa3d\xd7\x19O\xb5A\x10\x07" }

#[tokio::main]
async fn main() -> Result<()> {
    let our_ip = SocketAddrV4::from_str("108.61.166.203:63653")?;

    let our_node = NodeOwned {
        id: everscale_crypto::tl::PublicKeyOwned::Ed25519 { key: [196, 13, 241, 117, 234, 97, 249, 180, 215, 62, 2, 9, 77, 7, 242, 125, 42, 102, 193, 205, 211, 206, 89, 194, 25, 205, 130, 103, 70, 45, 126, 25] },
        overlay: [252, 6, 27, 161, 30, 29, 123, 169, 45, 198, 235, 37, 186, 121, 23, 74, 94, 164, 177, 30, 166, 41, 159, 156, 216, 13, 244, 33, 79, 29, 219, 59],
        version: 1718470331,
        signature: Bytes::from_static(b"f\xb3\0\x7f\xfd\xe02`m\x87$'\xbe\xaa\xa25WD\xb0\xf9\x8a\xbb\x8d.|\xe0\xe7\x01*\xe9N\x8fB\xcc_\xb1*\x8d\x04|\xe3\x0c\xc6\xea&\n\xc2\x16\xa1\xf7M\xfc\x81\xf8\xaf\xa3d\xd7\x19O\xb5A\x10\x07"),
    };

    tracing_subscriber::fmt::fmt().with_max_level(tracing::Level::TRACE).with_span_events(FmtSpan::CLOSE).init();

    const KEY_TAG: usize = 0;

    let global_config =
        serde_json::from_str::<global_config::GlobalConfig>(include_str!("ton-mainnet.json"))?;

    // Resolve public ip
    let my_ip = public_ip::addr_v4()
        .await
        .context("failed to resolve public ip address")?;

    // Create and fill keystore
    let keystore = adnl::Keystore::builder()
        .with_tagged_key(rand::thread_rng().gen(), KEY_TAG)?
        .build();

    // Create basic network parts
    let (adnl, dht, rldp, overlay) = NetworkBuilder::with_adnl(
        (my_ip, 0),
        keystore,
        everscale_network::adnl::NodeOptions {
            query_min_timeout_ms: 50,
            query_default_timeout_ms: 100,
            transfer_timeout_sec: 3,
            clock_tolerance_sec: 60,
            channel_reset_timeout_sec: 30,
            address_list_timeout_sec: 10,
            packet_history_enabled: false,
            packet_signature_required: false,
            force_use_priority_channels: false,
            use_loopback_for_neighbours: false,
            version: None,
        },
    )
    .with_dht(
        KEY_TAG,
        NodeOptions {
            value_ttl_sec: 60,
            query_timeout_ms: 100,
            default_value_batch_len: 3,
            bad_peer_threshold: 5,
            max_allowed_k: 5,
            max_key_name_len: 127,
            max_key_index: 15,
            storage_gc_interval_ms: 10000,
        },
    )
    .with_rldp(everscale_network::rldp::NodeOptions {
        max_answer_size: 1000000000,
        max_peer_queries: 10,
        query_min_timeout_ms: 100,
        query_max_timeout_ms: 500,
        query_wave_len: 5,
        query_wave_interval_ms: 50,
        force_compression: false,
    })
    .with_overlay(KEY_TAG)
    .build()?;

    // Fill static nodes
    // for peer in global_config.dht_nodes {
    //     dht.add_dht_peer(peer)?;
    // }

    // for _ in 0..3 {
    //     let new_dht_nodes = dht.find_more_dht_nodes().await?;
    //     tracing::info!("found {new_dht_nodes} DHT nodes");
    // }

    // Add masterchain overlay
    let mc_overlay_id =
        overlay::IdFull::for_workchain_overlay(-1, global_config.zero_state.file_hash.as_array())
            .compute_short_id();
    let (workchain_overlay, _) = overlay.add_public_overlay(
        &mc_overlay_id,
        OverlayOptions {
            max_neighbours: 100,
            max_broadcast_log: 100,
            broadcast_gc_interval_ms: 1000,
            overlay_peers_timeout_ms: 60000,
            max_ordinary_broadcast_len: 1000000,
            broadcast_target_count: 5,
            secondary_broadcast_target_count: 3,
            secondary_fec_broadcast_target_count: 3,
            fec_broadcast_wave_len: 5,
            fec_broadcast_wave_interval_ms: 50,
            broadcast_timeout_sec: 60,
            force_compression: false,
        },
    );

    // Populate overlay with nodes
    // let mut overlay_nodes = Vec::new();
    // let mut visited = HashSet::new();

    // let (our_ip, our_node) = 'outer: loop {
    //     for (ip, node) in &mut dht
    //         .find_overlay_nodes(&mc_overlay_id)
    //         .await
    //         .context("failed to find overlay nodes")?
    //     {
    //         let id = adnl::NodeIdFull::try_from(node.as_equivalent_ref().id)?.compute_short_id();
    //         if visited.contains(&id) {
    //             continue;
    //         }
    //         visited.insert(id);
    //         overlay_nodes.push((ip.clone(), node.clone()));
    //         if ip.ip() == &Ipv4Addr::from_str("108.61.166.203")? && ip.port() == 63653 {
    //             tracing::info!(?id, "our node <----------------");
    //             break 'outer (ip.clone(), node.clone());
    //         }
    //     }
    //     tracing::info!("found {} overlay nodes", visited.len());
    // };
    // tracing::info!(?our_ip, ?our_node, "our node <---------");

    if let Some(peer_id) =
        workchain_overlay.add_public_peer(&adnl, our_ip, our_node.as_equivalent_ref())?
    {
        tracing::info!("STARTING ADNL");
        let adnl_result = workchain_overlay
            .adnl_query(&adnl, &peer_id, RpcGetCapabilities, Some(1000))
            .await;
        tracing::info!("STARTING RLDP");
        let rldp_result = workchain_overlay
            .rldp_query(&rldp, &peer_id, RpcGetCapabilities, Some(1000))
            .await;
        tracing::info!(?peer_id, ?adnl_result, ?rldp_result, "FINISHED");
    }

    // Done
    Ok(())
}

#[derive(TlWrite, TlRead)]
#[tl(
    boxed,
    id = "tonNode.getCapabilities",
    scheme_inline = "tonNode.getCapabilities = tonNode.Capabilities;"
)]
pub struct RpcGetCapabilities;

#[derive(Debug, Copy, Clone, TlWrite, TlRead)]
#[tl(
    boxed,
    id = "tonNode.capabilities",
    size_hint = 12,
    scheme_inline = "tonNode.capabilities version:int capabilities:long = tonNode.Capabilities;"
)]
pub struct Capabilities {
    pub version: u32,
    pub capabilities: u64,
}
