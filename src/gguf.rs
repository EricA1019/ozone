use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologySource {
    GgufMetadata,
    SizeHeuristic,
}

impl TopologySource {
    pub fn label(self) -> &'static str {
        match self {
            TopologySource::GgufMetadata => "GGUF metadata",
            TopologySource::SizeHeuristic => "Size heuristic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTopology {
    pub total_layers: u32,
    pub architecture: Option<String>,
    pub source: TopologySource,
    pub note: Option<String>,
}

const GGUF_MAGIC: &[u8; 4] = b"GGUF";

const GGUF_TYPE_UINT8: u32 = 0;
const GGUF_TYPE_INT8: u32 = 1;
const GGUF_TYPE_UINT16: u32 = 2;
const GGUF_TYPE_INT16: u32 = 3;
const GGUF_TYPE_UINT32: u32 = 4;
const GGUF_TYPE_INT32: u32 = 5;
const GGUF_TYPE_FLOAT32: u32 = 6;
const GGUF_TYPE_BOOL: u32 = 7;
const GGUF_TYPE_STRING: u32 = 8;
const GGUF_TYPE_ARRAY: u32 = 9;
const GGUF_TYPE_UINT64: u32 = 10;
const GGUF_TYPE_INT64: u32 = 11;
const GGUF_TYPE_FLOAT64: u32 = 12;

pub fn inspect_model_topology(path: &Path, fallback_layers: u32) -> ModelTopology {
    match read_topology_from_metadata(path) {
        Ok(topology) => topology,
        Err(error) => ModelTopology {
            total_layers: fallback_layers,
            architecture: None,
            source: TopologySource::SizeHeuristic,
            note: Some(format!(
                "Could not read GGUF layer metadata, so Ozone fell back to file-size estimation ({error})."
            )),
        },
    }
}

fn read_topology_from_metadata(path: &Path) -> anyhow::Result<ModelTopology> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != GGUF_MAGIC {
        anyhow::bail!("file does not start with GGUF magic");
    }

    let version = read_u32(&mut reader)?;
    if !(2..=3).contains(&version) {
        anyhow::bail!("unsupported GGUF version {version}");
    }

    let _tensor_count = read_u64(&mut reader)?;
    let kv_count = read_u64(&mut reader)?;

    let mut architecture = None;
    let mut block_count = None;
    let mut fallback_block_count = None;

    for _ in 0..kv_count {
        let key = read_string(&mut reader)?;
        let value_type = read_u32(&mut reader)?;

        match value_type {
            GGUF_TYPE_STRING => {
                let value = read_string(&mut reader)?;
                if key == "general.architecture" {
                    architecture = Some(value);
                    if let (Some(arch), Some(count)) =
                        (architecture.as_deref(), fallback_block_count)
                    {
                        if key_matches_block_count(
                            &format!("{arch}.block_count"),
                            &format!("{arch}.n_layer"),
                            arch,
                            count,
                            &mut block_count,
                        ) {
                            // no-op; helper updates block_count
                        }
                    }
                }
            }
            GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 => {
                let value = read_numeric_value(&mut reader, value_type)?;
                if key.ends_with(".block_count") || key.ends_with(".n_layer") {
                    if let Ok(count) = u32::try_from(value) {
                        if let Some(arch) = architecture.as_deref() {
                            if key == format!("{arch}.block_count")
                                || key == format!("{arch}.n_layer")
                            {
                                block_count = Some(count);
                            } else if fallback_block_count.is_none() {
                                fallback_block_count = Some(count);
                            }
                        } else if fallback_block_count.is_none() {
                            fallback_block_count = Some(count);
                        }
                    }
                }
            }
            _ => skip_value(&mut reader, value_type)?,
        }

        if let Some(count) = block_count.or(fallback_block_count) {
            if count > 0 {
                return Ok(ModelTopology {
                    total_layers: count,
                    architecture,
                    source: TopologySource::GgufMetadata,
                    note: None,
                });
            }
        }
    }

    anyhow::bail!("GGUF metadata did not contain a usable block count");
}

fn key_matches_block_count(
    block_key: &str,
    layer_key: &str,
    _arch: &str,
    count: u32,
    slot: &mut Option<u32>,
) -> bool {
    if count == 0 {
        return false;
    }
    let _ = (block_key, layer_key);
    *slot = Some(count);
    true
}

fn read_numeric_value<R: Read>(reader: &mut R, value_type: u32) -> anyhow::Result<u64> {
    match value_type {
        GGUF_TYPE_UINT32 => Ok(u64::from(read_u32(reader)?)),
        GGUF_TYPE_INT32 => {
            let value = read_i32(reader)?;
            u64::try_from(value).map_err(|_| anyhow::anyhow!("negative int32 metadata value"))
        }
        GGUF_TYPE_UINT64 => read_u64(reader),
        GGUF_TYPE_INT64 => {
            let value = read_i64(reader)?;
            u64::try_from(value).map_err(|_| anyhow::anyhow!("negative int64 metadata value"))
        }
        _ => anyhow::bail!("unsupported numeric metadata type {value_type}"),
    }
}

fn skip_value<R: Read + Seek>(reader: &mut R, value_type: u32) -> anyhow::Result<()> {
    match value_type {
        GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => seek_forward(reader, 1),
        GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => seek_forward(reader, 2),
        GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => seek_forward(reader, 4),
        GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => seek_forward(reader, 8),
        GGUF_TYPE_STRING => {
            let len = read_u64(reader)?;
            seek_forward(reader, len)
        }
        GGUF_TYPE_ARRAY => {
            let element_type = read_u32(reader)?;
            let len = read_u64(reader)?;
            if let Some(width) = primitive_width(element_type) {
                seek_forward(reader, width.saturating_mul(len))
            } else {
                for _ in 0..len {
                    skip_value(reader, element_type)?;
                }
                Ok(())
            }
        }
        _ => anyhow::bail!("unsupported GGUF metadata type {value_type}"),
    }
}

fn primitive_width(value_type: u32) -> Option<u64> {
    match value_type {
        GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => Some(1),
        GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => Some(2),
        GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => Some(4),
        GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => Some(8),
        _ => None,
    }
}

fn seek_forward<R: Seek>(reader: &mut R, bytes: u64) -> anyhow::Result<()> {
    let offset = i64::try_from(bytes).map_err(|_| anyhow::anyhow!("metadata field too large"))?;
    reader.seek(SeekFrom::Current(offset))?;
    Ok(())
}

fn read_string<R: Read>(reader: &mut R) -> anyhow::Result<String> {
    let len = read_u64(reader)?;
    let len = usize::try_from(len).map_err(|_| anyhow::anyhow!("string length too large"))?;
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes)?;
    Ok(String::from_utf8(bytes)?)
}

fn read_u32<R: Read>(reader: &mut R) -> anyhow::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32<R: Read>(reader: &mut R) -> anyhow::Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u64<R: Read>(reader: &mut R) -> anyhow::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i64<R: Read>(reader: &mut R) -> anyhow::Result<i64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_gguf_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ozone-gguf-test-{name}-{}-{nanos}.gguf",
            std::process::id()
        ))
    }

    fn write_string(buf: &mut Vec<u8>, value: &str) {
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    fn write_kv_string(buf: &mut Vec<u8>, key: &str, value: &str) {
        write_string(buf, key);
        buf.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
        write_string(buf, value);
    }

    fn write_kv_u32(buf: &mut Vec<u8>, key: &str, value: u32) {
        write_string(buf, key);
        buf.extend_from_slice(&GGUF_TYPE_UINT32.to_le_bytes());
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn write_test_gguf(path: &Path, kv_writes: impl FnOnce(&mut Vec<u8>), kv_count: u64) {
        let mut buf = Vec::new();
        buf.extend_from_slice(GGUF_MAGIC);
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&kv_count.to_le_bytes());
        kv_writes(&mut buf);
        fs::write(path, buf).expect("write test gguf");
    }

    #[test]
    fn reads_block_count_from_metadata() {
        let path = temp_gguf_path("metadata");
        write_test_gguf(
            &path,
            |buf| {
                write_kv_string(buf, "general.architecture", "llama");
                write_kv_u32(buf, "llama.block_count", 40);
            },
            2,
        );

        let topology = inspect_model_topology(&path, 32);
        assert_eq!(topology.total_layers, 40);
        assert_eq!(topology.architecture.as_deref(), Some("llama"));
        assert_eq!(topology.source, TopologySource::GgufMetadata);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn falls_back_when_metadata_is_missing() {
        let path = temp_gguf_path("fallback");
        write_test_gguf(
            &path,
            |buf| write_kv_string(buf, "general.name", "sample"),
            1,
        );

        let topology = inspect_model_topology(&path, 48);
        assert_eq!(topology.total_layers, 48);
        assert_eq!(topology.source, TopologySource::SizeHeuristic);
        assert!(topology.note.is_some());

        let _ = fs::remove_file(path);
    }
}
