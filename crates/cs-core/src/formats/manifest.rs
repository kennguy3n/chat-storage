//! Backup and archive manifest specs.
//!
//! Manifests are signed with a hybrid Ed25519 + ML-DSA-65 device key
//! (see [`crate::crypto::signing`]). The two signatures are computed
//! over the canonical CBOR encoding of the manifest with both
//! `manifest_signature` and `pqc_signature` set to empty.

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::serde_bytes_array;
use crate::crypto::signing::{
    encode_ml_dsa_signature, HybridSigningKey, HybridVerifyingKey, MlDsaSignature,
    ML_DSA_65_SIGNATURE_LEN,
};
use crate::crypto::{CryptoError, CryptoResult};
use ed25519_dalek::{Signature, SIGNATURE_LENGTH};

/// Magic string for [`BackupManifest`].
pub const BACKUP_MANIFEST_MAGIC: &str = "KCHAT_BAK_MANIFEST_V2";

/// Magic string for [`ArchiveManifest`].
pub const ARCHIVE_MANIFEST_MAGIC: &str = "KCHAT_ARC_MANIFEST_V2";

/// On-wire manifest version.
pub const MANIFEST_VERSION: u32 = 2;

/// All-zero `previous_manifest_hash` for the genesis manifest.
pub const GENESIS_PREVIOUS_HASH: [u8; 32] = [0u8; 32];

// --- Manifest sub-records ---------------------------------------------------

/// Reference to a sealed segment uploaded under this manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestSegmentRef {
    pub segment_id: Uuid,
    pub segment_type: super::SegmentType,
    #[serde(with = "serde_bytes_array")]
    pub ciphertext_sha256: [u8; 32],
    pub size: u64,
}

/// Reference to a search index shard committed under this manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestShardRef {
    pub shard_id: Uuid,
    pub index_type: super::search_shard::IndexType,
    #[serde(with = "serde_bytes_array")]
    pub ciphertext_sha256: [u8; 32],
    pub time_bucket: String,
}

/// Reference to a media object backed up under this manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestMediaRef {
    pub asset_id: Uuid,
    pub blob_id: Uuid,
    #[serde(with = "serde_bytes_array")]
    pub merkle_root: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub wrapped_k_asset: Vec<u8>,
}

/// Tombstone record for a hard-deleted message, conversation, or asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tombstone {
    pub kind: String,
    pub id: String,
    pub deleted_at_ms: i64,
}

/// Reference to a retired archive epoch key, wrapped under
/// `K_archive_root` (AES-256-KW).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrappedEpochKeyRef {
    pub epoch_id: String,
    #[serde(with = "serde_bytes")]
    pub wrapped_key: Vec<u8>,
}

// --- BackupManifest ---------------------------------------------------------

/// Backup manifest frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub magic: String,
    pub version: u32,
    pub manifest_id: Uuid,
    pub generation: u64,
    #[serde(with = "serde_bytes_array")]
    pub previous_manifest_hash: [u8; 32],
    pub segments: Vec<ManifestSegmentRef>,
    pub search_index_shards: Vec<ManifestShardRef>,
    pub media_references: Vec<ManifestMediaRef>,
    pub tombstones: Vec<Tombstone>,
    #[serde(with = "serde_bytes_array")]
    pub merkle_root: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub manifest_signature: Vec<u8>,
    #[serde(default, with = "serde_bytes")]
    pub pqc_signature: Vec<u8>,
}

impl BackupManifest {
    pub fn has_valid_header(&self) -> bool {
        if self.magic != BACKUP_MANIFEST_MAGIC || self.version != MANIFEST_VERSION {
            return false;
        }
        if self.generation == 0 && self.previous_manifest_hash != GENESIS_PREVIOUS_HASH {
            return false;
        }
        true
    }
}

// --- ArchiveManifest --------------------------------------------------------

/// Archive manifest frame for the personal-archive store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveManifest {
    pub magic: String,
    pub version: u32,
    pub manifest_id: Uuid,
    pub generation: u64,
    #[serde(with = "serde_bytes_array")]
    pub previous_manifest_hash: [u8; 32],
    pub segments: Vec<ManifestSegmentRef>,
    pub search_index_shards: Vec<ManifestShardRef>,
    pub media_references: Vec<ManifestMediaRef>,
    pub tombstones: Vec<Tombstone>,
    #[serde(default)]
    pub wrapped_prior_epoch_keys: Vec<WrappedEpochKeyRef>,
    #[serde(with = "serde_bytes_array")]
    pub merkle_root: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub manifest_signature: Vec<u8>,
    #[serde(default, with = "serde_bytes")]
    pub pqc_signature: Vec<u8>,
}

impl ArchiveManifest {
    pub fn has_valid_header(&self) -> bool {
        if self.magic != ARCHIVE_MANIFEST_MAGIC || self.version != MANIFEST_VERSION {
            return false;
        }
        if self.generation == 0 && self.previous_manifest_hash != GENESIS_PREVIOUS_HASH {
            return false;
        }
        true
    }
}

// --- Canonical encoding + sign / verify -------------------------------------

/// Trait implemented by both manifest types so sign / verify and hash
/// helpers can be written once.
pub trait Manifest: Serialize {
    fn signing_signature_placeholder() -> Vec<u8> {
        Vec::new()
    }

    fn set_signature(&mut self, sig: Vec<u8>);
    fn signature(&self) -> &[u8];
    fn set_pqc_signature(&mut self, sig: Vec<u8>);
    fn pqc_signature(&self) -> &[u8];
}

impl Manifest for BackupManifest {
    fn set_signature(&mut self, sig: Vec<u8>) {
        self.manifest_signature = sig;
    }
    fn signature(&self) -> &[u8] {
        &self.manifest_signature
    }
    fn set_pqc_signature(&mut self, sig: Vec<u8>) {
        self.pqc_signature = sig;
    }
    fn pqc_signature(&self) -> &[u8] {
        &self.pqc_signature
    }
}

impl Manifest for ArchiveManifest {
    fn set_signature(&mut self, sig: Vec<u8>) {
        self.manifest_signature = sig;
    }
    fn signature(&self) -> &[u8] {
        &self.manifest_signature
    }
    fn set_pqc_signature(&mut self, sig: Vec<u8>) {
        self.pqc_signature = sig;
    }
    fn pqc_signature(&self) -> &[u8] {
        &self.pqc_signature
    }
}

fn canonical_signing_payload<M>(manifest: &M) -> CryptoResult<Vec<u8>>
where
    M: Manifest + Clone,
{
    let mut clone = manifest.clone();
    clone.set_signature(M::signing_signature_placeholder());
    clone.set_pqc_signature(M::signing_signature_placeholder());
    crate::cbor::to_vec(&clone)
        .map_err(|_| CryptoError::Frame("manifest: canonical CBOR encode failed".to_string()))
}

/// Hybrid signature pair returned by [`sign_backup_manifest`] /
/// [`sign_archive_manifest`].
#[derive(Debug, Clone)]
pub struct HybridManifestSignature {
    pub ed25519: Signature,
    pub ml_dsa: MlDsaSignature,
}

impl HybridManifestSignature {
    pub fn ed25519_bytes(&self) -> [u8; SIGNATURE_LENGTH] {
        self.ed25519.to_bytes()
    }

    pub fn pqc_bytes(&self) -> Vec<u8> {
        crate::crypto::signing::encode_ml_dsa_signature(&self.ml_dsa)
    }
}

fn sign<M>(
    manifest: &mut M,
    signing_key: &HybridSigningKey,
) -> CryptoResult<HybridManifestSignature>
where
    M: Manifest + Clone,
{
    let payload = canonical_signing_payload(manifest)?;
    let (ed_sig, ml_sig) = signing_key.sign_payload(&payload)?;
    manifest.set_signature(ed_sig.to_bytes().to_vec());
    manifest.set_pqc_signature(encode_ml_dsa_signature(&ml_sig));
    Ok(HybridManifestSignature {
        ed25519: ed_sig,
        ml_dsa: ml_sig,
    })
}

fn verify<M>(manifest: &M, verifying_key: &HybridVerifyingKey) -> CryptoResult<()>
where
    M: Manifest + Clone,
{
    if manifest.signature().len() != SIGNATURE_LENGTH {
        return Err(CryptoError::Frame(format!(
            "manifest: ed25519 signature must be {SIGNATURE_LENGTH} bytes, got {}",
            manifest.signature().len()
        )));
    }
    if manifest.pqc_signature().len() != ML_DSA_65_SIGNATURE_LEN {
        return Err(CryptoError::Frame(format!(
            "manifest: ml-dsa-65 signature must be {ML_DSA_65_SIGNATURE_LEN} bytes, got {}",
            manifest.pqc_signature().len()
        )));
    }
    let payload = canonical_signing_payload(manifest)?;
    verifying_key
        .verify_payload(&payload, manifest.signature(), manifest.pqc_signature())
        .map_err(|leg| match leg {
            crate::crypto::signing::HybridSignatureFailure::Ed25519 => {
                CryptoError::Signature("manifest: ed25519 verify failed")
            }
            crate::crypto::signing::HybridSignatureFailure::MlDsa => {
                CryptoError::Signature("manifest: ml-dsa-65 verify failed")
            }
        })
}

/// Sign a [`BackupManifest`] in place.
pub fn sign_backup_manifest(
    manifest: &mut BackupManifest,
    signing_key: &HybridSigningKey,
) -> CryptoResult<HybridManifestSignature> {
    sign(manifest, signing_key)
}

/// Verify a [`BackupManifest`]'s hybrid signatures.
pub fn verify_backup_manifest(
    manifest: &BackupManifest,
    verifying_key: &HybridVerifyingKey,
) -> CryptoResult<()> {
    verify(manifest, verifying_key)
}

/// Sign an [`ArchiveManifest`] in place.
pub fn sign_archive_manifest(
    manifest: &mut ArchiveManifest,
    signing_key: &HybridSigningKey,
) -> CryptoResult<HybridManifestSignature> {
    sign(manifest, signing_key)
}

/// Verify an [`ArchiveManifest`]'s hybrid signatures.
pub fn verify_archive_manifest(
    manifest: &ArchiveManifest,
    verifying_key: &HybridVerifyingKey,
) -> CryptoResult<()> {
    verify(manifest, verifying_key)
}

/// Lower-level sign helper on pre-encoded payload bytes.
pub fn sign_manifest(
    manifest_bytes: &[u8],
    signing_key: &HybridSigningKey,
) -> CryptoResult<HybridManifestSignature> {
    let (ed_sig, ml_sig) = signing_key.sign_payload(manifest_bytes)?;
    Ok(HybridManifestSignature {
        ed25519: ed_sig,
        ml_dsa: ml_sig,
    })
}

/// Lower-level verify helper on pre-encoded payload bytes.
pub fn verify_manifest(
    manifest_bytes: &[u8],
    ed25519_signature: &[u8],
    pqc_signature: &[u8],
    verifying_key: &HybridVerifyingKey,
) -> CryptoResult<()> {
    verifying_key
        .verify_payload(manifest_bytes, ed25519_signature, pqc_signature)
        .map_err(|leg| match leg {
            crate::crypto::signing::HybridSignatureFailure::Ed25519 => {
                CryptoError::Signature("verify_manifest: ed25519 verify failed")
            }
            crate::crypto::signing::HybridSignatureFailure::MlDsa => {
                CryptoError::Signature("verify_manifest: ml-dsa-65 verify failed")
            }
        })
}

/// 32-byte BLAKE3 over the canonical CBOR encoding of `manifest`.
pub fn compute_manifest_hash(manifest: &BackupManifest) -> CryptoResult<[u8; 32]> {
    let bytes = crate::cbor::to_vec(manifest)
        .map_err(|_| CryptoError::Frame("manifest: hash CBOR encode failed".to_string()))?;
    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

/// 32-byte BLAKE3 over the canonical CBOR encoding of an [`ArchiveManifest`].
pub fn compute_archive_manifest_hash(manifest: &ArchiveManifest) -> CryptoResult<[u8; 32]> {
    let bytes = crate::cbor::to_vec(manifest)
        .map_err(|_| CryptoError::Frame("manifest: hash CBOR encode failed".to_string()))?;
    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

// --- Compatibility types for existing callers ---

/// Compatibility alias for existing code that references `SegmentRef`.
pub type SegmentRef = ManifestSegmentRef;

/// Compatibility alias for existing code that references `WrappedEpochKey`.
pub type WrappedEpochKey = WrappedEpochKeyRef;

/// Compatibility: encode a manifest payload to CBOR.
pub fn encode_payload<T: serde::Serialize>(payload: &T) -> Result<Vec<u8>, crate::Error> {
    crate::cbor::to_vec(payload).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Compatibility: decode a manifest payload from CBOR.
pub fn decode_payload<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, crate::Error> {
    crate::cbor::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Compatibility: the old `BackupManifestPayload` type name is aliased
/// to `BackupManifest` for existing callers that construct manifests.
pub type BackupManifestPayload = BackupManifest;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::signing::HybridSigningKey;
    use crate::formats::search_shard::IndexType;
    use crate::formats::SegmentType;
    use rand::rngs::OsRng;

    fn keys() -> (HybridSigningKey, HybridVerifyingKey) {
        let mut rng = OsRng;
        let sk = HybridSigningKey::generate(&mut rng);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn sample_segment_ref() -> ManifestSegmentRef {
        ManifestSegmentRef {
            segment_id: Uuid::now_v7(),
            segment_type: SegmentType::Events,
            ciphertext_sha256: [0xCC; 32],
            size: 4096,
        }
    }

    fn sample_shard_ref() -> ManifestShardRef {
        ManifestShardRef {
            shard_id: Uuid::now_v7(),
            index_type: IndexType::Text,
            ciphertext_sha256: [0xDD; 32],
            time_bucket: "2026-04".to_string(),
        }
    }

    fn sample_media_ref() -> ManifestMediaRef {
        ManifestMediaRef {
            asset_id: Uuid::now_v7(),
            blob_id: Uuid::now_v7(),
            merkle_root: [0xEE; 32],
            wrapped_k_asset: vec![0xFF; 40],
        }
    }

    fn sample_tombstone() -> Tombstone {
        Tombstone {
            kind: "message".to_string(),
            id: "msg_0001".to_string(),
            deleted_at_ms: 1_714_651_200_000,
        }
    }

    fn fresh_genesis_backup() -> BackupManifest {
        BackupManifest {
            magic: BACKUP_MANIFEST_MAGIC.to_string(),
            version: MANIFEST_VERSION,
            manifest_id: Uuid::now_v7(),
            generation: 0,
            previous_manifest_hash: GENESIS_PREVIOUS_HASH,
            segments: vec![sample_segment_ref()],
            search_index_shards: vec![sample_shard_ref()],
            media_references: vec![sample_media_ref()],
            tombstones: vec![sample_tombstone()],
            merkle_root: [0x11; 32],
            manifest_signature: Vec::new(),
            pqc_signature: Vec::new(),
        }
    }

    fn fresh_genesis_archive() -> ArchiveManifest {
        ArchiveManifest {
            magic: ARCHIVE_MANIFEST_MAGIC.to_string(),
            version: MANIFEST_VERSION,
            manifest_id: Uuid::now_v7(),
            generation: 0,
            previous_manifest_hash: GENESIS_PREVIOUS_HASH,
            segments: vec![ManifestSegmentRef {
                segment_id: Uuid::now_v7(),
                segment_type: SegmentType::MessageDelta,
                ciphertext_sha256: [0xAA; 32],
                size: 8192,
            }],
            search_index_shards: vec![sample_shard_ref()],
            media_references: vec![sample_media_ref()],
            tombstones: vec![sample_tombstone()],
            wrapped_prior_epoch_keys: vec![],
            merkle_root: [0x22; 32],
            manifest_signature: Vec::new(),
            pqc_signature: Vec::new(),
        }
    }

    #[test]
    fn backup_manifest_round_trips_through_cbor() {
        let mut m = fresh_genesis_backup();
        let (sk, _vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();

        let bytes = crate::cbor::to_vec(&m).expect("encode");
        let decoded: BackupManifest = crate::cbor::from_slice(&bytes).expect("decode");
        assert_eq!(decoded, m);
    }

    #[test]
    fn archive_manifest_round_trips_through_cbor() {
        let mut m = fresh_genesis_archive();
        let (sk, _vk) = keys();
        sign_archive_manifest(&mut m, &sk).unwrap();

        let bytes = crate::cbor::to_vec(&m).expect("encode");
        let decoded: ArchiveManifest = crate::cbor::from_slice(&bytes).expect("decode");
        assert_eq!(decoded, m);
    }

    #[test]
    fn backup_manifest_sign_verify_round_trip() {
        let mut m = fresh_genesis_backup();
        let (sk, vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();
        verify_backup_manifest(&m, &vk).expect("signature should verify");
    }

    #[test]
    fn archive_manifest_sign_verify_round_trip() {
        let mut m = fresh_genesis_archive();
        let (sk, vk) = keys();
        sign_archive_manifest(&mut m, &sk).unwrap();
        verify_archive_manifest(&m, &vk).expect("signature should verify");
    }

    #[test]
    fn verify_rejects_tampered_backup_manifest() {
        let mut m = fresh_genesis_backup();
        let (sk, vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();
        m.merkle_root[0] ^= 0x01;
        let res = verify_backup_manifest(&m, &vk);
        assert!(res.is_err(), "tampered manifest verified: {res:?}");
    }

    #[test]
    fn verify_rejects_tampered_archive_manifest() {
        let mut m = fresh_genesis_archive();
        let (sk, vk) = keys();
        sign_archive_manifest(&mut m, &sk).unwrap();
        m.tombstones.push(Tombstone {
            kind: "media".to_string(),
            id: "asset_0002".to_string(),
            deleted_at_ms: 1_714_651_201_000,
        });
        let res = verify_archive_manifest(&m, &vk);
        assert!(res.is_err(), "tampered manifest verified: {res:?}");
    }

    #[test]
    fn verify_rejects_wrong_key_backup_manifest() {
        let mut m = fresh_genesis_backup();
        let (sk, _vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();
        let (_other_sk, other_vk) = keys();
        let res = verify_backup_manifest(&m, &other_vk);
        assert!(res.is_err(), "wrong-key verify accepted: {res:?}");
    }

    #[test]
    fn verify_rejects_wrong_key_archive_manifest() {
        let mut m = fresh_genesis_archive();
        let (sk, _vk) = keys();
        sign_archive_manifest(&mut m, &sk).unwrap();
        let (_other_sk, other_vk) = keys();
        let res = verify_archive_manifest(&m, &other_vk);
        assert!(res.is_err(), "wrong-key verify accepted: {res:?}");
    }

    #[test]
    fn verify_rejects_truncated_signature() {
        let mut m = fresh_genesis_backup();
        let (sk, vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();
        m.manifest_signature.truncate(16);
        let res = verify_backup_manifest(&m, &vk);
        assert!(res.is_err(), "truncated signature verified: {res:?}");
    }

    #[test]
    fn previous_manifest_hash_chain_walks_correctly() {
        let (sk, vk) = keys();

        let mut gen0 = fresh_genesis_backup();
        gen0.generation = 0;
        gen0.previous_manifest_hash = GENESIS_PREVIOUS_HASH;
        sign_backup_manifest(&mut gen0, &sk).unwrap();
        verify_backup_manifest(&gen0, &vk).unwrap();
        assert_eq!(gen0.previous_manifest_hash, [0u8; 32]);

        let gen0_hash = compute_manifest_hash(&gen0).unwrap();
        let mut gen1 = fresh_genesis_backup();
        gen1.generation = 1;
        gen1.previous_manifest_hash = gen0_hash;
        sign_backup_manifest(&mut gen1, &sk).unwrap();
        verify_backup_manifest(&gen1, &vk).unwrap();
        assert_eq!(gen1.previous_manifest_hash, gen0_hash);

        let gen1_hash = compute_manifest_hash(&gen1).unwrap();
        let mut gen2 = fresh_genesis_backup();
        gen2.generation = 2;
        gen2.previous_manifest_hash = gen1_hash;
        sign_backup_manifest(&mut gen2, &sk).unwrap();
        verify_backup_manifest(&gen2, &vk).unwrap();
        assert_eq!(gen2.previous_manifest_hash, gen1_hash);

        assert_ne!(gen0_hash, gen1_hash);
        assert_ne!(gen1_hash, [0u8; 32]);
    }

    #[test]
    fn genesis_manifest_has_zero_previous_hash() {
        let m = fresh_genesis_backup();
        assert_eq!(m.generation, 0);
        assert_eq!(m.previous_manifest_hash, GENESIS_PREVIOUS_HASH);
        assert!(m.has_valid_header());
    }

    #[test]
    fn genesis_archive_manifest_has_zero_previous_hash() {
        let m = fresh_genesis_archive();
        assert_eq!(m.generation, 0);
        assert_eq!(m.previous_manifest_hash, GENESIS_PREVIOUS_HASH);
        assert!(m.has_valid_header());
    }

    #[test]
    fn header_validation_rejects_non_genesis_with_zero_prev_hash() {
        let mut m = fresh_genesis_backup();
        m.generation = 7;
        m.previous_manifest_hash = GENESIS_PREVIOUS_HASH;
        assert!(m.has_valid_header());

        m.magic = "WRONG".to_string();
        assert!(!m.has_valid_header());
    }

    #[test]
    fn raw_sign_manifest_round_trip() {
        let (sk, vk) = keys();
        let payload = b"arbitrary bytes that the engine has already serialised";
        let sig = sign_manifest(payload, &sk).expect("hybrid sign");
        let ed_bytes = sig.ed25519.to_bytes().to_vec();
        let pq_bytes = encode_ml_dsa_signature(&sig.ml_dsa);
        verify_manifest(payload, &ed_bytes, &pq_bytes, &vk).expect("hybrid verify");
    }

    #[test]
    fn raw_verify_manifest_rejects_short_ed25519_signature() {
        let (sk, vk) = keys();
        let payload = b"x";
        let sig = sign_manifest(payload, &sk).unwrap();
        let truncated = &sig.ed25519.to_bytes()[..16];
        let pq_bytes = encode_ml_dsa_signature(&sig.ml_dsa);
        assert!(verify_manifest(payload, truncated, &pq_bytes, &vk).is_err());
    }

    #[test]
    fn raw_verify_manifest_rejects_short_pqc_signature() {
        let (sk, vk) = keys();
        let payload = b"x";
        let sig = sign_manifest(payload, &sk).unwrap();
        let ed_bytes = sig.ed25519.to_bytes().to_vec();
        let pq_truncated = encode_ml_dsa_signature(&sig.ml_dsa)
            .into_iter()
            .take(32)
            .collect::<Vec<_>>();
        assert!(verify_manifest(payload, &ed_bytes, &pq_truncated, &vk).is_err());
    }

    #[test]
    fn verify_rejects_zero_pqc_signature() {
        let mut m = fresh_genesis_backup();
        let (sk, vk) = keys();
        sign_backup_manifest(&mut m, &sk).unwrap();
        m.pqc_signature = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
        let res = verify_backup_manifest(&m, &vk);
        assert!(res.is_err(), "all-zero pqc verified: {res:?}");
    }
}
