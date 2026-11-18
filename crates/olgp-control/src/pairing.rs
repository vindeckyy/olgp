//! Pairing state machine built on the vendored Lumen Noise/SPAKE2 primitives.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use lumen_crypto::handshake::Role;
use serde::{Deserialize, Serialize};
use lumen_crypto::pairing::{
    confirmation_matches, host_proof, pairing_proof, random_bytes, ChannelBinding,
    Confirmations, DeviceIdentity, PairingCode, PairingPake, KEY_LEN, PAKE_MESSAGE_LEN,
    SIGNATURE_LEN,
};
use thiserror::Error;

use crate::discovery::BoxInfo;

/// Pairing failures.
#[derive(Debug, Error)]
pub enum PairingError {
    #[error("pairing exchange is not in the expected state")]
    BadState,
    #[error("client identity proof is invalid")]
    BadProof,
    #[error("host confirmation is invalid")]
    BadConfirmation,
    #[error("crypto error: {0}")]
    Crypto(String),
}

impl From<lumen_crypto::error::CryptoError> for PairingError {
    fn from(error: lumen_crypto::error::CryptoError) -> Self {
        Self::Crypto(error.to_string())
    }
}

/// The credential stored by a paired client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Credential {
    pub box_id: String,
    pub token: String,
    pub host_key: String,
    pub host_signature: String,
    pub box_spki_sha256: String,
    pub capabilities: Vec<String>,
    pub paired_at: String,
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 15)] as char);
    }
    out
}

fn unhex(text: &str) -> Result<Vec<u8>, PairingError> {
    if text.len() % 2 != 0 {
        return Err(PairingError::Crypto("odd hex length".into()));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .map_err(|_| PairingError::Crypto("bad hex".into()))
        })
        .collect()
}

/// Host side of one pairing attempt.
pub struct HostPairing {
    identity: DeviceIdentity,
    code: PairingCode,
    nonce: [u8; 32],
    binding: ChannelBinding,
    pake: Option<PairingPake>,
}

impl HostPairing {
    /// Start a fresh attempt for this connection.
    #[must_use]
    pub fn new(identity: DeviceIdentity, binding: ChannelBinding) -> Self {
        Self {
            identity,
            code: PairingCode::random(),
            nonce: random_bytes::<32>(),
            binding,
            pake: None,
        }
    }

    /// Six-digit code to display on the box.
    #[must_use]
    pub fn code(&self) -> String {
        self.code.to_string()
    }

    /// Nonce sent to the client for its identity proof.
    #[must_use]
    pub fn nonce(&self) -> &[u8; 32] {
        &self.nonce
    }

    /// Begin SPAKE2 as the host (side B).
    #[must_use]
    pub fn begin(&mut self) -> [u8; PAKE_MESSAGE_LEN] {
        let (state, message) = PairingPake::start(Role::Responder, self.code, &self.binding);
        self.pake = Some(state);
        message
    }

    /// Validate the client's proof and SPAKE2 confirmation.
    ///
    /// # Errors
    /// Returns an error on a bad proof, wrong code, or invalid exchange state.
    pub fn finish(
        &mut self,
        client_public: &[u8; KEY_LEN],
        client_pake: &[u8; PAKE_MESSAGE_LEN],
        client_proof: &[u8; SIGNATURE_LEN],
        box_info: &BoxInfo,
    ) -> Result<(Confirmations, Credential), PairingError> {
        let pake = self.pake.take().ok_or(PairingError::BadState)?;
        let expected = pairing_proof(
            &self.nonce,
            &self.identity.public_bytes(),
            client_public,
            &self.binding,
        );
        DeviceIdentity::verify_bytes(client_public, &expected, client_proof)
            .map_err(|_| PairingError::BadProof)?;
        let confirmations = pake.finish(client_pake)?;
        let proof = host_proof(
            &self.identity.public_bytes(),
            client_public,
            &self.binding,
        );
        let signature = self.identity.sign_bytes(&proof);
        let credential = Credential {
            box_id: box_info.box_id.clone(),
            token: hex(&random_bytes::<32>()),
            host_key: hex(&self.identity.public_bytes()),
            host_signature: hex(&signature),
            box_spki_sha256: box_info.spki_sha256.clone(),
            capabilities: box_info.capabilities.clone(),
            paired_at: "2026-01-01T00:00:00Z".into(),
        };
        Ok((confirmations, credential))
    }
}

/// Client side of one pairing attempt.
pub struct ClientPairing {
    identity: DeviceIdentity,
    binding: ChannelBinding,
    pake: Option<PairingPake>,
}

impl ClientPairing {
    /// Start SPAKE2 as the client (side A).
    #[must_use]
    pub fn start(
        identity: DeviceIdentity,
        code: PairingCode,
        binding: ChannelBinding,
    ) -> (Self, [u8; PAKE_MESSAGE_LEN]) {
        let (state, message) = PairingPake::start(Role::Initiator, code, &binding);
        (
            Self {
                identity,
                binding,
                pake: Some(state),
            },
            message,
        )
    }

    /// Build the proof bytes sent to the host.
    #[must_use]
    pub fn proof(&self, nonce: &[u8; 32], host_public: &[u8; KEY_LEN]) -> Vec<u8> {
        pairing_proof(
            nonce,
            host_public,
            &self.identity.public_bytes(),
            &self.binding,
        )
    }

    /// Verify the host signature and SPAKE2 confirmation.
    ///
    /// # Errors
    /// Returns an error on a bad proof, bad confirmation, or malformed hex.
    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &mut self,
        host_pake: &[u8; PAKE_MESSAGE_LEN],
        host_confirmation: &[u8; 32],
        credential: Credential,
    ) -> Result<(Confirmations, Credential), PairingError> {
        let pake = self.pake.take().ok_or(PairingError::BadState)?;
        let confirmations = pake.finish(host_pake)?;
        if !confirmation_matches(&confirmations.host, host_confirmation) {
            return Err(PairingError::BadConfirmation);
        }
        let host_public = unhex(&credential.host_key)?;
        let host_signature = unhex(&credential.host_signature)?;
        if host_public.len() != KEY_LEN || host_signature.len() != SIGNATURE_LEN {
            return Err(PairingError::BadProof);
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&host_public);
        let mut signature = [0u8; SIGNATURE_LEN];
        signature.copy_from_slice(&host_signature);
        let proof = host_proof(&key, &self.identity.public_bytes(), &self.binding);
        DeviceIdentity::verify_bytes(&key, &proof, &signature)
            .map_err(|_| PairingError::BadProof)?;
        Ok((confirmations, credential))
    }
}

/// Store a credential with owner-only permissions.
///
/// # Errors
/// Returns an IO error when the file cannot be written.
pub fn store_credential(path: &Path, credential: &Credential) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = format!(
        "{{\n  \"box_id\": \"{}\",\n  \"token\": \"{}\",\n  \"host_key\": \"{}\",\n  \"host_signature\": \"{}\",\n  \"box_spki_sha256\": \"{}\",\n  \"capabilities\": [{}],\n  \"paired_at\": \"{}\"\n}}\n",
        credential.box_id,
        credential.token,
        credential.host_key,
        credential.host_signature,
        credential.box_spki_sha256,
        credential
            .capabilities
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", "),
        credential.paired_at
    );
    fs::write(path, text)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::short_fingerprint;
    use std::net::Ipv4Addr;

    use std::net::IpAddr;
    fn box_info() -> BoxInfo {
        BoxInfo {
            box_id: "a".repeat(32),
            spki_sha256: "b".repeat(64),
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            api_port: 8787,
            rpc_port: 8843,
            capabilities: vec!["cuda".into()],
            gpu: "GPU".into(),
            vram_mib: 4096,
            hostname: "box".into(),
        }
    }

    #[test]
    fn full_pairing_exchange_succeeds() {
        let host_identity = DeviceIdentity::generate().unwrap();
        let client_identity = DeviceIdentity::generate().unwrap();
        let binding = ChannelBinding {
            client_static: [1u8; 32],
            host_static: [2u8; 32],
        };
        let mut host = HostPairing::new(host_identity.clone(), binding);
        let code = PairingCode::parse(&host.code()).unwrap();
        let (mut client, client_pake) = ClientPairing::start(client_identity.clone(), code, binding);
        let host_pake = host.begin();
        let proof = client.proof(host.nonce(), &host_identity.public_bytes());
        let signature = client_identity.sign_bytes(&proof);
        let (confirmations, credential) = host
            .finish(
                &client_identity.public_bytes(),
                &client_pake,
                &signature,
                &box_info(),
            )
            .unwrap();
        let (_, verified) = client
            .finish(&host_pake, &confirmations.host, credential)
            .unwrap();
        assert_eq!(verified.box_id, box_info().box_id);
        assert_eq!(short_fingerprint(&verified.box_spki_sha256).len(), 27);
    }

    #[test]
    fn wrong_confirmation_fails() {
        let binding = ChannelBinding {
            client_static: [3u8; 32],
            host_static: [4u8; 32],
        };
        let host_identity = DeviceIdentity::generate().unwrap();
        let mut host = HostPairing::new(host_identity.clone(), binding);
        let host_pake = host.begin();
        let client_identity = DeviceIdentity::generate().unwrap();
        let (mut client, client_pake) =
            ClientPairing::start(client_identity.clone(), PairingCode::new(111_111), binding);
        let proof = client.proof(host.nonce(), &host_identity.public_bytes());
        let signature = client_identity.sign_bytes(&proof);
        let (_, credential) = host
            .finish(
                &client_identity.public_bytes(),
                &client_pake,
                &signature,
                &box_info(),
            )
            .unwrap();
        assert!(client.finish(&host_pake, &[0u8; 32], credential).is_err());
    }

    #[test]
    fn store_credential_writes_owner_only_file() {
        let root = std::env::temp_dir().join(format!("olgp-cred-{}", std::process::id()));
        let path = root.join("cred.json");
        let credential = Credential {
            box_id: "a".repeat(32),
            token: "token".into(),
            host_key: "ab".repeat(32),
            host_signature: "cd".repeat(64),
            box_spki_sha256: "ef".repeat(32),
            capabilities: vec!["cuda".into()],
            paired_at: "now".into(),
        };
        store_credential(&path, &credential).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"token\": \"token\""));
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unhex_rejects_odd_length() {
        assert!(unhex("abc").is_err());
    }

    #[test]
    fn host_finish_without_begin_fails() {
        let binding = ChannelBinding {
            client_static: [9u8; 32],
            host_static: [8u8; 32],
        };
        let mut host = HostPairing::new(DeviceIdentity::generate().unwrap(), binding);
        let result = host.finish(&[1u8; 32], &[2u8; 33], &[3u8; 64], &box_info());
        assert!(matches!(result, Err(PairingError::BadState)));
    }
}
