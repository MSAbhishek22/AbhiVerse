//! Chunked, resumable, hash-verified file transfer (Task 005).
//!
//! A file is split into fixed-size chunks. The whole file's SHA-256
//! is declared before transfer; every receiver re-computes it after
//! assembly, so integrity never depends on trusting the relay.
//! Received chunks are tracked explicitly, which makes resume a
//! matter of "send only what's missing".

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Fixed chunk size for all transfers (256 KiB).
pub const CHUNK_SIZE: u64 = 256 * 1024;

pub fn sha256_hex(data: &[u8]) -> String {
    abhiverse_identity::hex_encode(&Sha256::digest(data))
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(abhiverse_identity::hex_encode(&hasher.finalize()))
}

/// Number of chunks for a file of `size` bytes. An empty file has
/// zero chunks and is verified by the empty-content hash alone.
pub fn total_chunks(size: u64) -> u32 {
    if size == 0 {
        0
    } else {
        size.div_ceil(CHUNK_SIZE) as u32
    }
}

/// Expected byte length of chunk `index` for a file of `size` bytes.
pub fn chunk_len(size: u64, index: u32) -> u64 {
    let start = index as u64 * CHUNK_SIZE;
    if start >= size {
        0
    } else {
        (size - start).min(CHUNK_SIZE)
    }
}

/// A file name stripped of any directory components. Rejects names
/// that would escape a target directory.
pub fn sanitize_file_name(name: &str) -> Option<String> {
    let base = name.replace('\\', "/");
    let base = base.rsplit('/').next()?;
    if base.is_empty() || base == "." || base == ".." || base.contains('\0') {
        return None;
    }
    Some(base.to_string())
}

/// Tracks which chunks of a download are already on disk, persisted
/// as JSON so an interrupted download resumes where it stopped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReceiveProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub received: Vec<u32>,
}

impl ReceiveProgress {
    pub fn new(transfer_id: &str, file_name: &str, size_bytes: u64, sha256: &str) -> Self {
        Self {
            transfer_id: transfer_id.to_string(),
            file_name: file_name.to_string(),
            size_bytes,
            sha256: sha256.to_string(),
            received: Vec::new(),
        }
    }

    pub fn missing(&self) -> Vec<u32> {
        let total = total_chunks(self.size_bytes);
        (0..total).filter(|i| !self.received.contains(i)).collect()
    }

    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }

    pub fn load(path: &Path) -> Option<Self> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

/// The `.part` file a download is assembled into.
pub fn part_path(dir: &Path, transfer_id: &str) -> PathBuf {
    dir.join(format!("{transfer_id}.part"))
}

/// Write one chunk at its correct offset in the `.part` file.
pub fn write_chunk(dir: &Path, transfer_id: &str, index: u32, data: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = part_path(dir, transfer_id);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    f.seek(SeekFrom::Start(index as u64 * CHUNK_SIZE))?;
    f.write_all(data)?;
    Ok(())
}

/// Finish a download: verify the assembled file's hash, then rename
/// it to its final (sanitised) name. On hash mismatch the `.part`
/// file is kept for inspection/resume and an error is returned.
pub fn finish_download(dir: &Path, progress: &ReceiveProgress) -> std::io::Result<PathBuf> {
    let part = part_path(dir, &progress.transfer_id);
    let actual = sha256_file(&part)?;
    if actual != progress.sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "SHA-256 mismatch: expected {}, assembled {}",
                progress.sha256, actual
            ),
        ));
    }
    let name = sanitize_file_name(&progress.file_name)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "unsafe file name"))?;
    let final_path = dir.join(name);
    std::fs::rename(&part, &final_path)?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_math() {
        assert_eq!(total_chunks(0), 0);
        assert_eq!(total_chunks(1), 1);
        assert_eq!(total_chunks(CHUNK_SIZE), 1);
        assert_eq!(total_chunks(CHUNK_SIZE + 1), 2);
        assert_eq!(chunk_len(CHUNK_SIZE + 5, 0), CHUNK_SIZE);
        assert_eq!(chunk_len(CHUNK_SIZE + 5, 1), 5);
        assert_eq!(chunk_len(10, 3), 0);
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(
            sanitize_file_name("../../etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(sanitize_file_name("a/b/c.txt").as_deref(), Some("c.txt"));
        assert_eq!(sanitize_file_name(".."), None);
        assert_eq!(sanitize_file_name(""), None);
        assert_eq!(
            sanitize_file_name("plain.txt").as_deref(),
            Some("plain.txt")
        );
    }

    #[test]
    fn assemble_out_of_order_and_verify() {
        let dir = std::env::temp_dir().join(format!("abhiverse-ft-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // 300_000 bytes of deterministic pseudo-random data (3 chunks? no: 2 chunks)
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let hash = sha256_hex(&data);
        let mut progress = ReceiveProgress::new("t1", "blob.bin", data.len() as u64, &hash);
        let total = total_chunks(data.len() as u64);
        assert_eq!(total, 2);
        for index in (0..total).rev() {
            let start = (index as u64 * CHUNK_SIZE) as usize;
            let end = ((index as u64 * CHUNK_SIZE) + chunk_len(data.len() as u64, index)) as usize;
            write_chunk(&dir, "t1", index, &data[start..end]).unwrap();
            progress.received.push(index);
        }
        assert!(progress.is_complete());
        let out = finish_download(&dir, &progress).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), data);

        // Resume bookkeeping: missing() lists only absent chunks
        let mut p2 = ReceiveProgress::new("t2", "x", CHUNK_SIZE * 3, "00");
        p2.received = vec![0, 2];
        assert_eq!(p2.missing(), vec![1]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finish_rejects_corrupt_assembly() {
        let dir = std::env::temp_dir().join(format!("abhiverse-ft-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let progress = ReceiveProgress::new("t9", "f.bin", 5, &sha256_hex(b"hello"));
        write_chunk(&dir, "t9", 0, b"HELLO").unwrap(); // wrong bytes
        let err = finish_download(&dir, &progress).unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
