//! Benchmarks criterion — anti-régression perf du décodage filaire (D33).
//!
//! Le décodage est sur le chemin chaud de CHAQUE paquet reçu : une régression y
//! coûte à tout le trafic. On mesure les enveloppes les plus fréquentes
//! (DATA applicatif, message de canal) et une liste DHT plafonnée.
//!
//! Exécution : `cargo bench -p accord-proto --bench decode`.

use accord_proto::core_msg::{CoreMsg, MsgBody};
use accord_proto::dht_msg::{DhtBody, DhtMessage};
use accord_proto::*;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::net::SocketAddr;

fn node(i: u8) -> NodeInfo {
    NodeInfo {
        node_id: NodeId([i; 32]),
        static_pub: [i.wrapping_add(1); 32],
        pow_nonce: u64::from(i),
        flags: types::node_flags::RELAY,
        addrs: vec![WireAddr("192.168.1.10:4433".parse::<SocketAddr>().unwrap())],
    }
}

fn bench_decode_packet(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_packet_data");
    for taille in [64usize, 1024, 16 * 1024] {
        let bytes = Packet::Data(DataPacket {
            session_id: [7; 8],
            epoch: 1,
            counter: 42,
            ciphertext: vec![0xAB; taille],
        })
        .to_bytes();
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(taille), &bytes, |b, bytes| {
            b.iter(|| Packet::from_bytes(black_box(bytes)).unwrap())
        });
    }
    group.finish();
}

fn bench_decode_core(c: &mut Criterion) {
    // Message texte direct : la charge la plus fréquente d'une conversation.
    let corps = MsgBody::Text {
        text: "bonjour, ceci est un message de longueur réaliste".into(),
        reply_to: None,
        attachments: Vec::new(),
    };
    let dm = CoreMsg::DirectMsg {
        msg_id: [3; 16],
        lamport: 7,
        sent_ms: 1_700_000_000_000,
        kind: corps.kind(),
        body: corps.encode_body(),
    };
    let bytes = ChannelMsg::Core(dm).to_bytes();
    c.bench_function("decode_channel_core_dm", |b| {
        b.iter(|| ChannelMsg::from_bytes(black_box(&bytes)).unwrap())
    });
}

fn bench_decode_dht_list(c: &mut Criterion) {
    // Réponse FIND_NODE pleine (liste de nœuds plafonnée à DHT_K).
    let nodes: Vec<NodeInfo> = (0..limits::DHT_K as u8).map(node).collect();
    let bytes = DhtMessage {
        rpc_id: [1; 20],
        body: DhtBody::FoundNodes { nodes },
    }
    .to_bytes();
    c.bench_function("decode_dht_found_nodes_pleine", |b| {
        b.iter(|| DhtMessage::from_bytes(black_box(&bytes)).unwrap())
    });
}

criterion_group!(
    benches,
    bench_decode_packet,
    bench_decode_core,
    bench_decode_dht_list
);
criterion_main!(benches);
