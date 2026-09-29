//! Mutually authenticated terminal protocol v2. Trust anchors are provisioned out of band.
use aes_gcm::{Aes256Gcm, Nonce, aead::{Aead, KeyInit, Payload}};
use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use redash_types::{E2eeHandshakeAck, E2eeHandshakeInit, EncryptedEnvelope};
use sha2::{Digest, Sha256};

pub fn random_hex() -> String {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes).expect("OS cryptographic random source unavailable");
    hex::encode(bytes)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    hex::decode(s).map_err(|e| e.to_string())?.try_into().map_err(|_| format!("Expected {N} bytes"))
}

fn ephemeral() -> Result<([u8; 32], String), String> {
    let mut secret = [0; 32];
    getrandom::getrandom(&mut secret).map_err(|e| e.to_string())?;
    Ok((secret, hex::encode(MontgomeryPoint::mul_base_clamped(secret).as_bytes())))
}

fn verify(key: &str, bytes: &[u8], signature: &str) -> Result<(), String> {
    let key = VerifyingKey::from_bytes(&decode(key)?).map_err(|e| e.to_string())?;
    key.verify_strict(bytes, &ed25519_dalek::Signature::from_bytes(&decode(signature)?))
        .map_err(|_| "Identity signature verification failed".to_string())
}

struct Cipher {
    session_id: String,
    tx: Aes256Gcm,
    rx: Aes256Gcm,
    tx_direction: u8,
    tx_seq: u64,
    rx_seq: u64,
}

impl Cipher {
    fn new(secret: [u8; 32], peer: &str, init: &E2eeHandshakeInit, ack: &E2eeHandshakeAck, client: bool) -> Result<Self, String> {
        let shared = MontgomeryPoint(decode(peer)?).mul_clamped(secret).to_bytes();
        if shared == [0; 32] { return Err("Invalid low-order ECDH public key".into()); }
        let transcript = Sha256::digest(ack.signable_bytes(init));
        let kdf = Hkdf::<Sha256>::new(Some(&transcript), &shared);
        let mut c2a = [0; 32];
        let mut a2c = [0; 32];
        kdf.expand(b"redash-v2-client-to-agent", &mut c2a).map_err(|e| e.to_string())?;
        kdf.expand(b"redash-v2-agent-to-client", &mut a2c).map_err(|e| e.to_string())?;
        let (tx, rx) = if client { (c2a, a2c) } else { (a2c, c2a) };
        Ok(Self { session_id: init.session_id.clone(), tx: Aes256Gcm::new(&tx.into()), rx: Aes256Gcm::new(&rx.into()), tx_direction: if client { 1 } else { 2 }, tx_seq: 0, rx_seq: 0 })
    }

    fn nonce(direction: u8, seq: u64) -> [u8; 12] {
        let mut nonce = [0; 12];
        nonce[0] = direction;
        nonce[4..].copy_from_slice(&seq.to_be_bytes());
        nonce
    }

    fn aad(&self, direction: u8, seq: u64) -> Vec<u8> {
        serde_json::to_vec(&(2, &self.session_id, direction, seq)).expect("fixed envelope schema")
    }

    fn seal(&mut self, plaintext: &[u8]) -> Result<EncryptedEnvelope, String> {
        let seq = self.tx_seq.checked_add(1).ok_or("Sequence exhausted")?;
        let nonce = Self::nonce(self.tx_direction, seq);
        let mut encrypted = self.tx.encrypt(&Nonce::from(nonce), Payload { msg: plaintext, aad: &self.aad(self.tx_direction, seq) }).map_err(|_| "Encryption failed")?;
        let tag = encrypted.split_off(encrypted.len() - 16);
        self.tx_seq = seq;
        Ok(EncryptedEnvelope::new(&self.session_id, seq, hex::encode(nonce), hex::encode(encrypted), hex::encode(tag)))
    }

    fn open(&mut self, env: &EncryptedEnvelope) -> Result<Vec<u8>, String> {
        let direction = if self.tx_direction == 1 { 2 } else { 1 };
        if env.session_id != self.session_id || env.seq_num <= self.rx_seq {
            return Err("Wrong session or replayed/out-of-order frame".into());
        }
        let nonce = Self::nonce(direction, env.seq_num);
        if decode::<12>(&env.nonce_hex)? != nonce { return Err("Invalid nonce or direction".into()); }
        if env.ciphertext_hex.len() > 4 * 1024 * 1024 { return Err("Frame exceeds size limit".into()); }
        let mut encrypted = hex::decode(&env.ciphertext_hex).map_err(|e| e.to_string())?;
        encrypted.extend_from_slice(&decode::<16>(&env.tag_hex)?);
        let plaintext = self.rx.decrypt(&Nonce::from(nonce), Payload { msg: &encrypted, aad: &self.aad(direction, env.seq_num) }).map_err(|_| "AEAD authentication failed")?;
        self.rx_seq = env.seq_num;
        Ok(plaintext)
    }
}

pub struct ClientE2eeSession {
    pub session_id: String,
    secret: [u8; 32],
    init: E2eeHandshakeInit,
    trusted_agent: String,
    cipher: Option<Cipher>,
}

impl ClientE2eeSession {
    pub fn initiate(session_id: &str, node_id: &str, client_private_key: &str, trusted_agent: &str) -> Result<(Self, E2eeHandshakeInit), String> {
        VerifyingKey::from_bytes(&decode(trusted_agent)?).map_err(|e| e.to_string())?;
        let signing = SigningKey::from_bytes(&decode(client_private_key)?);
        let (secret, public) = ephemeral()?;
        let mut init = E2eeHandshakeInit { version: 2, node_id: node_id.into(), session_id: session_id.into(), client_ephemeral_pubkey_hex: public, timestamp: now_secs(), nonce: random_hex(), signature_hex: String::new() };
        init.signature_hex = hex::encode(signing.sign(&init.signable_bytes()).to_bytes());
        Ok((Self { session_id: session_id.into(), secret, init: init.clone(), trusted_agent: trusted_agent.into(), cipher: None }, init))
    }

    pub fn complete_handshake(&mut self, ack: &E2eeHandshakeAck) -> Result<(), String> {
        if self.cipher.is_some() || ack.session_id != self.session_id || !ack.success {
            return Err("Unexpected or rejected E2EE acknowledgement".into());
        }
        verify(&self.trusted_agent, &ack.signable_bytes(&self.init), &ack.signature_hex)?;
        self.cipher = Some(Cipher::new(self.secret, &ack.agent_ephemeral_pubkey_hex, &self.init, ack, true)?);
        self.secret.fill(0);
        Ok(())
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<EncryptedEnvelope, String> {
        self.cipher.as_mut().ok_or("Handshake incomplete")?.seal(plaintext)
    }
    pub fn open(&mut self, envelope: &EncryptedEnvelope) -> Result<Vec<u8>, String> {
        self.cipher.as_mut().ok_or("Handshake incomplete")?.open(envelope)
    }
}

pub struct AgentE2eeSession { cipher: Cipher }

impl AgentE2eeSession {
    pub fn respond(init: &E2eeHandshakeInit, trusted_client: &str, agent_private_key: &str, node_id: &str) -> Result<(Self, E2eeHandshakeAck), String> {
        let now = now_secs();
        if init.version != 2 || init.node_id != node_id || init.session_id.is_empty() || init.session_id.len() > 128 || init.nonce.len() != 64 || init.timestamp > now.saturating_add(30) || now.saturating_sub(init.timestamp) > 60 {
            return Err("Invalid or expired handshake context".into());
        }
        verify(trusted_client, &init.signable_bytes(), &init.signature_hex)?;
        let (secret, public) = ephemeral()?;
        let signing = SigningKey::from_bytes(&decode(agent_private_key)?);
        let mut ack = E2eeHandshakeAck { session_id: init.session_id.clone(), agent_ephemeral_pubkey_hex: public, signature_hex: String::new(), success: true, error_msg: None };
        ack.signature_hex = hex::encode(signing.sign(&ack.signable_bytes(init)).to_bytes());
        let cipher = Cipher::new(secret, &init.client_ephemeral_pubkey_hex, init, &ack, false)?;
        Ok((Self { cipher }, ack))
    }
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<EncryptedEnvelope, String> { self.cipher.seal(plaintext) }
    pub fn open(&mut self, envelope: &EncryptedEnvelope) -> Result<Vec<u8>, String> { self.cipher.open(envelope) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::ClientSigner;

    fn pair() -> (ClientE2eeSession, AgentE2eeSession) {
        let (cp, cs) = ClientSigner::generate_keypair();
        let (ap, ass) = ClientSigner::generate_keypair();
        let (mut client, init) = ClientE2eeSession::initiate("session", "node", &cs, &ap).unwrap();
        let (agent, ack) = AgentE2eeSession::respond(&init, &cp, &ass, "node").unwrap();
        client.complete_handshake(&ack).unwrap();
        (client, agent)
    }
    #[test]
    fn authenticated_roundtrip_rejects_replay_reflection_and_tamper() {
        let (mut client, mut agent) = pair();
        let env = client.seal(b"command").unwrap();
        let mut bad = env.clone(); bad.ciphertext_hex = "00".repeat(7);
        assert!(agent.open(&bad).is_err());
        assert_eq!(agent.open(&env).unwrap(), b"command");
        assert!(agent.open(&env).is_err());
        let out = agent.seal(b"output").unwrap();
        assert!(agent.open(&out).is_err());
        assert_eq!(client.open(&out).unwrap(), b"output");
        assert!(client.open(&out).is_err());
        let mut wrong = client.seal(b"next").unwrap(); wrong.session_id = "other".into();
        assert!(agent.open(&wrong).is_err());
    }
    #[test]
    fn agent_identity_and_full_transcript_are_required() {
        let (cp, cs) = ClientSigner::generate_keypair();
        let (ap, ass) = ClientSigner::generate_keypair();
        let (_, attacker) = ClientSigner::generate_keypair();
        let (mut client, init) = ClientE2eeSession::initiate("session", "node", &cs, &ap).unwrap();
        let (_, forged) = AgentE2eeSession::respond(&init, &cp, &attacker, "node").unwrap();
        assert!(client.complete_handshake(&forged).is_err());
        assert!(AgentE2eeSession::respond(&init, &cp, &ass, "other-node").is_err());
        let (_, mut ack) = AgentE2eeSession::respond(&init, &cp, &ass, "node").unwrap();
        ack.session_id = "other-session".into();
        assert!(client.complete_handshake(&ack).is_err());
        let mut expired = init; expired.timestamp = 0;
        assert!(AgentE2eeSession::respond(&expired, &cp, &ass, "node").is_err());
        assert!(client.seal(b"no plaintext fallback").is_err());
    }
}
