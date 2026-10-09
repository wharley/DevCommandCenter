//! Cross-implementation vectors.
//!
//! - `cacophony.json` holds the public Noise vectors (cacophony) for the two
//!   patterns DCC uses; snow must reproduce them byte for byte, and so must
//!   `packages/remote-protocol`.
//! - `snow-interop.json` is generated here with snow and DCC's own envelope,
//!   handshake payloads and fragmentation; the TypeScript tests replay it.
//!   The test fails when the committed file drifts from what snow produces;
//!   regenerate it with `DCC_REMOTE_WRITE_VECTORS=1 cargo test -p dcc-remote`.

use std::path::PathBuf;

use dcc_remote::channel::{SecureChannel, MAX_CHUNK};
use dcc_remote::noise::{ClientHello, DeviceInfo, Mode, ServerHello, PROLOGUE};
use dcc_remote::offer::{pair_id, psk_from_otp};
use dcc_remote::wire;
use dcc_remote::PROTOCOL_VERSION;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use snow::{Builder, HandshakeState};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/remote-protocol/test/vectors")
}

fn unhex(value: &Value) -> Vec<u8> {
    hex::decode(value.as_str().expect("hex string")).expect("valid hex")
}

#[test]
fn snow_matches_the_cacophony_vectors() {
    let text = std::fs::read_to_string(vectors_dir().join("cacophony.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let vectors = file["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 2);
    for vector in vectors {
        let name = vector["protocol_name"].as_str().unwrap();
        let init_static = unhex(&vector["init_static"]);
        let init_ephemeral = unhex(&vector["init_ephemeral"]);
        let init_remote = unhex(&vector["init_remote_static"]);
        let resp_static = unhex(&vector["resp_static"]);
        let resp_ephemeral = unhex(&vector["resp_ephemeral"]);
        let prologue = unhex(&vector["init_prologue"]);
        let psk: Option<[u8; 32]> = vector["init_psks"]
            .as_array()
            .and_then(|psks| psks.first())
            .map(|psk| unhex(psk).try_into().unwrap());

        let mut initiator = Builder::new(name.parse().unwrap())
            .prologue(&prologue)
            .unwrap()
            .local_private_key(&init_static)
            .unwrap()
            .remote_public_key(&init_remote)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&init_ephemeral);
        let mut responder = Builder::new(name.parse().unwrap())
            .prologue(&prologue)
            .unwrap()
            .local_private_key(&resp_static)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&resp_ephemeral);
        if let Some(psk) = psk.as_ref() {
            initiator = initiator.psk(2, psk).unwrap();
            responder = responder.psk(2, psk).unwrap();
        }
        let mut initiator = initiator.build_initiator().unwrap();
        let mut responder = responder.build_responder().unwrap();

        let messages = vector["messages"].as_array().unwrap();
        let mut buffer = vec![0u8; 65535];
        let mut out = vec![0u8; 65535];
        for message in &messages[..2] {
            let (writer, reader): (&mut HandshakeState, &mut HandshakeState) =
                if initiator.is_my_turn() {
                    (&mut initiator, &mut responder)
                } else {
                    (&mut responder, &mut initiator)
                };
            let payload = unhex(&message["payload"]);
            let len = writer.write_message(&payload, &mut buffer).unwrap();
            assert_eq!(hex::encode(&buffer[..len]), message["ciphertext"], "{name}");
            let read = reader.read_message(&buffer[..len], &mut out).unwrap();
            assert_eq!(&out[..read], payload.as_slice());
        }
        assert_eq!(
            hex::encode(initiator.get_handshake_hash()),
            vector["handshake_hash"],
            "{name}"
        );
        let mut initiator = initiator.into_transport_mode().unwrap();
        let mut responder = responder.into_transport_mode().unwrap();
        for (index, message) in messages[2..].iter().enumerate() {
            let payload = unhex(&message["payload"]);
            let (writer, reader) = if index % 2 == 0 {
                (&mut initiator, &mut responder)
            } else {
                (&mut responder, &mut initiator)
            };
            let len = writer.write_message(&payload, &mut buffer).unwrap();
            assert_eq!(hex::encode(&buffer[..len]), message["ciphertext"], "{name}");
            let read = reader.read_message(&buffer[..len], &mut out).unwrap();
            assert_eq!(&out[..read], payload.as_slice());
        }
    }
}

fn key(label: &str) -> [u8; 32] {
    Sha256::digest(format!("dcc-remote vectors {label}").as_bytes()).into()
}

fn public_of(private: &[u8; 32]) -> [u8; 32] {
    x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(*private)).to_bytes()
}

/// Byte `i` of a generated message; the TypeScript tests use the same rule.
fn generated(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

enum Payload {
    Text(String),
    Generated(usize),
}

impl Payload {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Payload::Text(text) => text.as_bytes().to_vec(),
            Payload::Generated(len) => generated(*len),
        }
    }

    fn describe(&self) -> Value {
        match self {
            Payload::Text(text) => json!({ "text": text }),
            Payload::Generated(len) => json!({ "generated": len }),
        }
    }
}

fn describe_wire(bytes: &[u8]) -> Value {
    if bytes.len() <= 256 {
        json!({ "hex": hex::encode(bytes) })
    } else {
        json!({
            "len": bytes.len(),
            "sha256": hex::encode(Sha256::digest(bytes)),
            "headHex": hex::encode(&bytes[..32]),
        })
    }
}

fn build_case(name: &str, mode: Mode) -> Value {
    let phone_static = key(&format!("{name} phone static"));
    let phone_ephemeral = key(&format!("{name} phone ephemeral"));
    let desktop_static = key(&format!("{name} desktop static"));
    let desktop_ephemeral = key(&format!("{name} desktop ephemeral"));
    let desktop_public = public_of(&desktop_static);
    let otp: [u8; 16] = key(&format!("{name} otp"))[..16].try_into().unwrap();
    let psk = psk_from_otp(&otp);
    let pairing = mode == Mode::Pair;

    let client_hello = ClientHello {
        v: PROTOCOL_VERSION,
        pair_id: pairing.then(|| pair_id(&otp)),
        device: Some(DeviceInfo {
            name: "Pixel do Teste ✓".into(),
            platform: Some("android".into()),
        }),
    };
    let server_hello = ServerHello {
        v: PROTOCOL_VERSION,
        device_id: (!pairing).then(|| "3f2c5b9e-0000-4000-8000-000000000001".to_string()),
    };

    let mut initiator = Builder::new(mode.pattern().parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&phone_static)
        .unwrap()
        .remote_public_key(&desktop_public)
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&phone_ephemeral);
    if pairing {
        initiator = initiator.psk(2, &psk).unwrap();
    }
    let mut initiator = initiator.build_initiator().unwrap();
    let mut responder = Builder::new(mode.pattern().parse().unwrap())
        .prologue(PROLOGUE)
        .unwrap()
        .local_private_key(&desktop_static)
        .unwrap()
        .fixed_ephemeral_key_for_testing_only(&desktop_ephemeral)
        .build_responder()
        .unwrap();

    let mut buffer = vec![0u8; 65535];
    let mut out = vec![0u8; 65535];
    let hello_bytes = serde_json::to_vec(&client_hello).unwrap();
    let len = initiator.write_message(&hello_bytes, &mut buffer).unwrap();
    let init_wire = wire::handshake_init(mode, &buffer[..len]);
    responder.read_message(&buffer[..len], &mut out).unwrap();
    if pairing {
        // The desktop picks the PSK only after reading message 1.
        responder.set_psk(2, &psk).unwrap();
    }
    let server_bytes = serde_json::to_vec(&server_hello).unwrap();
    let len = responder.write_message(&server_bytes, &mut buffer).unwrap();
    let response_wire = wire::handshake_response(&buffer[..len]);
    initiator.read_message(&buffer[..len], &mut out).unwrap();
    let handshake_hash = hex::encode(initiator.get_handshake_hash());
    assert_eq!(
        initiator.get_handshake_hash(),
        responder.get_handshake_hash()
    );

    let mut phone = SecureChannel::new(initiator.into_transport_mode().unwrap());
    let mut desktop = SecureChannel::new(responder.into_transport_mode().unwrap());
    let first = if pairing {
        r#"{"t":"ping"}"#.to_string()
    } else {
        r#"{"t":"req","id":1,"method":"GET","path":"/api/v1/sessions"}"#.to_string()
    };
    let second = if pairing {
        r#"{"t":"pairing","status":"pending"}"#.to_string()
    } else {
        r#"{"t":"res","id":1,"status":200,"body":"[]"}"#.to_string()
    };
    let transport = [
        ("phone", Payload::Text(first)),
        ("desktop", Payload::Text(second)),
        ("desktop", Payload::Generated(MAX_CHUNK * 2 + 5)),
        ("phone", Payload::Generated(MAX_CHUNK + 1)),
        ("phone", Payload::Text(String::new())),
    ];
    let mut steps = vec![
        json!({ "from": "phone", "handshake": describe_wire(&init_wire) }),
        json!({ "from": "desktop", "handshake": describe_wire(&response_wire) }),
    ];
    for (from, payload) in transport {
        let bytes = payload.bytes();
        let (sender, receiver) = if from == "phone" {
            (&mut phone, &mut desktop)
        } else {
            (&mut desktop, &mut phone)
        };
        let wires = sender.seal(&bytes).unwrap();
        let mut opened = None;
        for wire in &wires {
            opened = receiver.open_wire(wire).unwrap();
        }
        assert_eq!(opened.as_deref(), Some(bytes.as_slice()));
        steps.push(json!({
            "from": from,
            "message": payload.describe(),
            "wires": wires.iter().map(|wire| describe_wire(wire)).collect::<Vec<_>>(),
        }));
    }

    json!({
        "name": name,
        "mode": mode.to_wire(),
        "protocolName": mode.pattern(),
        "prologue": hex::encode(PROLOGUE),
        "otp": hex::encode(otp),
        "psk": hex::encode(psk),
        "pairId": pair_id(&otp),
        "phoneStaticPrivate": hex::encode(phone_static),
        "phoneStaticPublic": hex::encode(public_of(&phone_static)),
        "phoneEphemeralPrivate": hex::encode(phone_ephemeral),
        "desktopStaticPrivate": hex::encode(desktop_static),
        "desktopStaticPublic": hex::encode(desktop_public),
        "desktopEphemeralPrivate": hex::encode(desktop_ephemeral),
        "clientHello": String::from_utf8(hello_bytes).unwrap(),
        "serverHello": String::from_utf8(server_bytes).unwrap(),
        "handshakeHash": handshake_hash,
        "steps": steps,
    })
}

#[test]
fn snow_interop_vectors_are_current() {
    let generated = json!({
        "generator": "crates/dcc-remote/tests/vectors.rs (snow); regenerate with DCC_REMOTE_WRITE_VECTORS=1 cargo test -p dcc-remote",
        "generatedMessageRule": "byte i = (i * 31 + 7) % 251",
        "cases": [build_case("pair", Mode::Pair), build_case("resume", Mode::Resume)],
    });
    let text = serde_json::to_string_pretty(&generated).unwrap() + "\n";
    let path = vectors_dir().join("snow-interop.json");
    if std::env::var_os("DCC_REMOTE_WRITE_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == text,
        "snow-interop.json is stale; run DCC_REMOTE_WRITE_VECTORS=1 cargo test -p dcc-remote"
    );
}
