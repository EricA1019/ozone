use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::{EmbeddingProviderMetadata, EmbeddingRecord, MemoryArtifactId};

const VECTOR_INDEX_FORMAT: &str = "ozone-memory.vector-index.v1";
const VECTOR_INDEX_VERSION: u32 = 1;
const VECTOR_INDEX_FILE_NAME: &str = "memory.usearch";
const VECTOR_INDEX_METADATA_FILE_NAME: &str = "memory.usearch.json";
const VECTOR_INDEX_FILE_NAME_TMP: &str = "memory.usearch.next";
const VECTOR_INDEX_METADATA_FILE_NAME_TMP: &str = "memory.usearch.json.next";
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexPaths {
    root: PathBuf,
}

impl VectorIndexPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join(VECTOR_INDEX_FILE_NAME)
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.root.join(VECTOR_INDEX_METADATA_FILE_NAME)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorIndexMetadata {
    pub format: String,
    pub version: u32,
    pub provider: crate::EmbeddingProviderKind,
    pub model: String,
    pub dimensions: usize,
    pub artifact_count: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexState {
    pub metadata: VectorIndexMetadata,
    pub vector_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorIndexQueryMatch {
    pub key: u64,
    pub distance: f32,
    pub similarity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorIndexQueryResult {
    pub metadata: VectorIndexMetadata,
    pub matches: Vec<VectorIndexQueryMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexRebuildSummary {
    pub paths: VectorIndexPaths,
    pub metadata: VectorIndexMetadata,
}

#[derive(Debug, Error)]
pub enum VectorIndexError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to encode index metadata: {0}")]
    MetadataJson(#[from] serde_json::Error),
    #[error("vector index backend error: {0}")]
    Backend(String),
    #[error("invalid vector index metadata: {0}")]
    InvalidMetadata(String),
    #[error("invalid embedding record: {0}")]
    InvalidRecord(String),
    #[error("invalid query vector: {0}")]
    InvalidQuery(String),
}

pub struct VectorIndexManager {
    paths: VectorIndexPaths,
}

impl std::fmt::Debug for VectorIndexManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorIndexManager")
            .field("paths", &self.paths)
            .finish()
    }
}

impl VectorIndexManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            paths: VectorIndexPaths::new(root),
        }
    }

    pub fn paths(&self) -> &VectorIndexPaths {
        &self.paths
    }

    pub fn load_metadata(&self) -> Result<Option<VectorIndexMetadata>, VectorIndexError> {
        let metadata_path = self.paths.metadata_path();
        let index_path = self.paths.index_path();

        match (metadata_path.exists(), index_path.exists()) {
            (false, false) => return Ok(None),
            (false, true) => {
                return Err(VectorIndexError::InvalidMetadata(format!(
                    "index file exists without metadata at {}",
                    index_path.display()
                )))
            }
            (true, false) => {
                return Err(VectorIndexError::InvalidMetadata(format!(
                    "metadata exists without index file at {}",
                    metadata_path.display()
                )))
            }
            (true, true) => {}
        }

        let metadata = serde_json::from_slice::<VectorIndexMetadata>(&fs::read(metadata_path)?)?;
        validate_metadata(&metadata)?;
        Ok(Some(metadata))
    }

    pub fn open(&self) -> Result<Option<VectorIndexState>, VectorIndexError> {
        let Some(metadata) = self.load_metadata()? else {
            return Ok(None);
        };
        let index = create_index(metadata.provider_metadata())?;
        let index_path = self.paths.index_path();
        index
            .load(index_path.to_string_lossy().as_ref())
            .map_err(|error| VectorIndexError::Backend(error.to_string()))?;
        let vector_count = index.size();
        if vector_count != metadata.artifact_count {
            return Err(VectorIndexError::InvalidMetadata(format!(
                "metadata count {} does not match persisted index count {}",
                metadata.artifact_count, vector_count
            )));
        }

        Ok(Some(VectorIndexState {
            metadata,
            vector_count,
        }))
    }

    pub fn query(
        &self,
        query_vector: &[f32],
        count: usize,
    ) -> Result<Option<VectorIndexQueryResult>, VectorIndexError> {
        let Some(metadata) = self.load_metadata()? else {
            return Ok(None);
        };

        if query_vector.len() != metadata.dimensions {
            return Err(VectorIndexError::InvalidQuery(format!(
                "query vector had {} dimensions, expected {}",
                query_vector.len(),
                metadata.dimensions
            )));
        }

        let index = create_index(metadata.provider_metadata())?;
        let index_path = self.paths.index_path();
        index
            .load(index_path.to_string_lossy().as_ref())
            .map_err(|error| VectorIndexError::Backend(error.to_string()))?;

        let match_count = count.min(metadata.artifact_count);
        let matches = if match_count == 0 {
            Vec::new()
        } else {
            let raw_matches = index
                .exact_search(query_vector, match_count)
                .map_err(|error| VectorIndexError::Backend(error.to_string()))?;
            raw_matches
                .keys
                .into_iter()
                .zip(raw_matches.distances)
                .map(|(key, distance)| VectorIndexQueryMatch {
                    key,
                    distance,
                    similarity: (1.0 - distance).clamp(0.0, 1.0),
                })
                .collect()
        };

        Ok(Some(VectorIndexQueryResult { metadata, matches }))
    }

    pub fn rebuild(
        &self,
        provider: &EmbeddingProviderMetadata,
        records: &[EmbeddingRecord],
    ) -> Result<VectorIndexRebuildSummary, VectorIndexError> {
        validate_provider(provider)?;
        let sorted_records = sort_records(records);
        let index = create_index(provider.clone())?;
        if !sorted_records.is_empty() {
            index
                .reserve(sorted_records.len())
                .map_err(|error| VectorIndexError::Backend(error.to_string()))?;
        }

        let mut seen_keys = BTreeMap::new();
        for record in &sorted_records {
            validate_record(provider, record)?;
            let key = artifact_index_key(&record.artifact_id);
            if let Some(existing) = seen_keys.insert(key, record.artifact_id.as_str().to_owned()) {
                return Err(VectorIndexError::InvalidRecord(format!(
                    "artifact key collision between {existing} and {}",
                    record.artifact_id
                )));
            }
            index
                .add(key, &record.content.vector)
                .map_err(|error| VectorIndexError::Backend(error.to_string()))?;
        }

        fs::create_dir_all(self.paths.root())?;
        let metadata = VectorIndexMetadata {
            format: VECTOR_INDEX_FORMAT.to_owned(),
            version: VECTOR_INDEX_VERSION,
            provider: provider.provider,
            model: provider.model.clone(),
            dimensions: provider.dimensions,
            artifact_count: sorted_records.len(),
            fingerprint: fingerprint_records(provider, &sorted_records),
        };
        let metadata_json = serde_json::to_vec_pretty(&metadata)?;

        let index_tmp_path = self.paths.root().join(VECTOR_INDEX_FILE_NAME_TMP);
        let metadata_tmp_path = self.paths.root().join(VECTOR_INDEX_METADATA_FILE_NAME_TMP);
        remove_if_present(&index_tmp_path)?;
        remove_if_present(&metadata_tmp_path)?;
        index
            .save(index_tmp_path.to_string_lossy().as_ref())
            .map_err(|error| VectorIndexError::Backend(error.to_string()))?;
        fs::write(&metadata_tmp_path, metadata_json)?;
        replace_file(&index_tmp_path, &self.paths.index_path())?;
        replace_file(&metadata_tmp_path, &self.paths.metadata_path())?;

        Ok(VectorIndexRebuildSummary {
            paths: self.paths.clone(),
            metadata,
        })
    }
}

impl VectorIndexMetadata {
    pub fn provider_metadata(&self) -> EmbeddingProviderMetadata {
        EmbeddingProviderMetadata {
            provider: self.provider,
            model: self.model.clone(),
            dimensions: self.dimensions,
        }
    }
}

pub fn artifact_index_key(artifact_id: &MemoryArtifactId) -> u64 {
    let mut hasher = StableHasher::new(FNV_OFFSET_BASIS ^ 0x1f23_4ac5_b79d_0021);
    hasher.write_bytes(artifact_id.as_str().as_bytes());
    hasher.finish()
}

fn create_index(provider: EmbeddingProviderMetadata) -> Result<Index, VectorIndexError> {
    let options = IndexOptions {
        dimensions: provider.dimensions,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        ..IndexOptions::default()
    };
    Index::new(&options).map_err(|error| VectorIndexError::Backend(error.to_string()))
}

fn validate_provider(provider: &EmbeddingProviderMetadata) -> Result<(), VectorIndexError> {
    if provider.dimensions == 0 {
        return Err(VectorIndexError::InvalidMetadata(
            "vector dimensions must be greater than zero".to_owned(),
        ));
    }
    if provider.model.trim().is_empty() {
        return Err(VectorIndexError::InvalidMetadata(
            "embedding model must not be empty".to_owned(),
        ));
    }
    Ok(())
}

fn validate_metadata(metadata: &VectorIndexMetadata) -> Result<(), VectorIndexError> {
    if metadata.format != VECTOR_INDEX_FORMAT {
        return Err(VectorIndexError::InvalidMetadata(format!(
            "unsupported metadata format `{}`",
            metadata.format
        )));
    }
    if metadata.version != VECTOR_INDEX_VERSION {
        return Err(VectorIndexError::InvalidMetadata(format!(
            "unsupported metadata version {}",
            metadata.version
        )));
    }
    validate_provider(&metadata.provider_metadata())
}

fn validate_record(
    provider: &EmbeddingProviderMetadata,
    record: &EmbeddingRecord,
) -> Result<(), VectorIndexError> {
    if record.metadata != provider.clone().into() {
        return Err(VectorIndexError::InvalidRecord(format!(
            "embedding artifact {} metadata ({}/{}/{}) did not match rebuild provider ({}/{}/{})",
            record.artifact_id,
            record.metadata.provider,
            record.metadata.model,
            record.metadata.dimensions,
            provider.provider,
            provider.model,
            provider.dimensions
        )));
    }
    if record.content.vector.len() != provider.dimensions {
        return Err(VectorIndexError::InvalidRecord(format!(
            "embedding artifact {} had {} dimensions, expected {}",
            record.artifact_id,
            record.content.vector.len(),
            provider.dimensions
        )));
    }
    Ok(())
}

fn sort_records(records: &[EmbeddingRecord]) -> Vec<&EmbeddingRecord> {
    let mut sorted = records.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| {
        left.session_id
            .as_str()
            .cmp(right.session_id.as_str())
            .then_with(|| {
                left.source_message_id
                    .as_ref()
                    .map(|message_id| message_id.as_str())
                    .cmp(
                        &right
                            .source_message_id
                            .as_ref()
                            .map(|message_id| message_id.as_str()),
                    )
            })
            .then_with(|| left.snapshot_version.cmp(&right.snapshot_version))
            .then_with(|| left.artifact_id.as_str().cmp(right.artifact_id.as_str()))
    });
    sorted
}

fn fingerprint_records(
    provider: &EmbeddingProviderMetadata,
    records: &[&EmbeddingRecord],
) -> String {
    let mut hi = StableHasher::new(FNV_OFFSET_BASIS ^ 0x6d0f_27bd_4d20_7f57);
    let mut lo = StableHasher::new(FNV_OFFSET_BASIS ^ 0xa453_96ce_f157_ef21);
    let mut write_shared = |bytes: &[u8]| {
        hi.write_bytes(bytes);
        lo.write_bytes(bytes);
    };

    write_shared(provider.provider.as_str().as_bytes());
    write_shared(provider.model.as_bytes());
    write_shared(&provider.dimensions.to_le_bytes());
    write_shared(&records.len().to_le_bytes());

    for record in records {
        write_shared(record.artifact_id.as_str().as_bytes());
        write_shared(record.session_id.as_str().as_bytes());
        match record.source_message_id.as_ref() {
            Some(message_id) => {
                write_shared(&[1]);
                write_shared(message_id.as_str().as_bytes());
            }
            None => write_shared(&[0]),
        }
        write_shared(record.provenance.as_str().as_bytes());
        write_shared(&record.created_at.to_le_bytes());
        write_shared(&record.snapshot_version.to_le_bytes());
        write_shared(&record.content.source_text_hash.to_le_bytes());
        write_shared(&record.content.vector.len().to_le_bytes());
        for value in &record.content.vector {
            write_shared(&value.to_bits().to_le_bytes());
        }
    }

    format!("{:016x}{:016x}", hi.finish(), lo.finish())
}

fn remove_if_present(path: &Path) -> Result<(), VectorIndexError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VectorIndexError::Io(error)),
    }
}

fn replace_file(from: &Path, to: &Path) -> Result<(), VectorIndexError> {
    remove_if_present(to)?;
    fs::rename(from, to)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct StableHasher {
    state: u64,
}

impl StableHasher {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    fn finish(self) -> u64 {
        self.state
    }
}

impl From<EmbeddingProviderMetadata> for crate::EmbeddingRecordMetadata {
    fn from(value: EmbeddingProviderMetadata) -> Self {
        Self {
            provider: value.provider,
            model: value.model,
            dimensions: value.dimensions,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use ozone_core::{engine::MessageId, session::SessionId};

    use super::*;
    use crate::{
        source_text_hash, EmbeddingContent, EmbeddingProviderKind, EmbeddingRecordMetadata,
        Provenance,
    };

    static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn vector_index_round_trips_metadata_and_count() {
        let sandbox = TestSandbox::new("vector-index-roundtrip");
        let manager = VectorIndexManager::new(sandbox.root.join("index"));
        let provider = mock_provider_metadata();
        let records = vec![
            embedding_record(
                "123e4567-e89b-12d3-a456-426614174000",
                "223e4567-e89b-12d3-a456-426614174000",
                Some("323e4567-e89b-12d3-a456-426614174000"),
                vec![1.0, 0.0, 0.0],
                "Remember the observatory key.",
                1_725_647_200_000,
                1,
            ),
            embedding_record(
                "423e4567-e89b-12d3-a456-426614174000",
                "223e4567-e89b-12d3-a456-426614174000",
                None,
                vec![0.0, 1.0, 0.0],
                "Pack the brass lantern.",
                1_725_647_200_100,
                1,
            ),
        ];

        let summary = manager.rebuild(&provider, &records).unwrap();
        assert!(summary.paths.index_path().exists());
        assert!(summary.paths.metadata_path().exists());
        assert_eq!(summary.metadata.artifact_count, 2);
        assert_eq!(summary.metadata.dimensions, 3);
        assert_eq!(summary.metadata.provider, EmbeddingProviderKind::Mock);

        let metadata = manager.load_metadata().unwrap().unwrap();
        assert_eq!(metadata, summary.metadata);

        let state = manager.open().unwrap().unwrap();
        assert_eq!(state.vector_count, 2);
        assert_eq!(state.metadata, summary.metadata);
    }

    #[test]
    fn vector_index_rebuild_is_deterministic_for_same_records() {
        let sandbox = TestSandbox::new("vector-index-determinism");
        let manager = VectorIndexManager::new(sandbox.root.join("index"));
        let provider = mock_provider_metadata();
        let first = embedding_record(
            "523e4567-e89b-12d3-a456-426614174000",
            "623e4567-e89b-12d3-a456-426614174000",
            Some("723e4567-e89b-12d3-a456-426614174000"),
            vec![0.0, 0.0, 1.0],
            "The gate opens at dusk.",
            1_725_647_200_000,
            1,
        );
        let second = embedding_record(
            "823e4567-e89b-12d3-a456-426614174000",
            "623e4567-e89b-12d3-a456-426614174000",
            None,
            vec![0.0, 1.0, 0.0],
            "Carry the spare lens.",
            1_725_647_200_100,
            2,
        );

        let first_summary = manager
            .rebuild(&provider, &[first.clone(), second.clone()])
            .unwrap();
        let second_summary = manager.rebuild(&provider, &[second, first]).unwrap();

        assert_eq!(first_summary.metadata, second_summary.metadata);
        assert_eq!(
            manager.open().unwrap().unwrap().metadata.fingerprint,
            first_summary.metadata.fingerprint
        );
    }

    #[test]
    fn vector_index_query_returns_exact_neighbors() {
        let sandbox = TestSandbox::new("vector-index-query");
        let manager = VectorIndexManager::new(sandbox.root.join("index"));
        let provider = mock_provider_metadata();
        let first = embedding_record(
            "923e4567-e89b-12d3-a456-426614174000",
            "a23e4567-e89b-12d3-a456-426614174000",
            Some("b23e4567-e89b-12d3-a456-426614174000"),
            vec![1.0, 0.0, 0.0],
            "observatory",
            1_725_647_200_000,
            1,
        );
        let second = embedding_record(
            "c23e4567-e89b-12d3-a456-426614174000",
            "a23e4567-e89b-12d3-a456-426614174000",
            Some("d23e4567-e89b-12d3-a456-426614174000"),
            vec![0.0, 1.0, 0.0],
            "lantern",
            1_725_647_200_100,
            2,
        );

        manager
            .rebuild(&provider, &[first.clone(), second.clone()])
            .unwrap();
        let result = manager
            .query(&[0.9, 0.1, 0.0], 2)
            .unwrap()
            .expect("index should exist");

        assert_eq!(result.matches.len(), 2);
        assert_eq!(
            result.matches[0].key,
            artifact_index_key(&first.artifact_id)
        );
        assert!(result.matches[0].similarity >= result.matches[1].similarity);
        assert_eq!(result.metadata.provider, EmbeddingProviderKind::Mock);
    }

    fn mock_provider_metadata() -> EmbeddingProviderMetadata {
        EmbeddingProviderMetadata {
            provider: EmbeddingProviderKind::Mock,
            model: "mock/stable".to_owned(),
            dimensions: 3,
        }
    }

    fn embedding_record(
        artifact_id: &str,
        session_id: &str,
        source_message_id: Option<&str>,
        vector: Vec<f32>,
        text: &str,
        created_at: i64,
        snapshot_version: u64,
    ) -> EmbeddingRecord {
        EmbeddingRecord {
            artifact_id: MemoryArtifactId::parse(artifact_id).unwrap(),
            session_id: SessionId::parse(session_id).unwrap(),
            content: EmbeddingContent::new(vector, source_text_hash(text)),
            source_message_id: source_message_id.map(|value| MessageId::parse(value).unwrap()),
            provenance: Provenance::UserAuthored,
            created_at,
            snapshot_version,
            metadata: EmbeddingRecordMetadata {
                provider: EmbeddingProviderKind::Mock,
                model: "mock/stable".to_owned(),
                dimensions: 3,
            },
        }
    }

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ozone-memory-tests")
                .join(format!(
                    "{prefix}-{}-{}",
                    std::process::id(),
                    SANDBOX_COUNTER.fetch_add(1, Ordering::Relaxed)
                ));
            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
