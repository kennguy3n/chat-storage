//! Media cache — encrypted file cache for media originals, thumbnails, keyframes.

use std::path::PathBuf;

/// Encrypted media file cache.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MediaCache {
    cache_dir: PathBuf,
    max_bytes: u64,
    current_bytes: u64,
}

impl MediaCache {
    pub fn new(cache_dir: PathBuf, max_bytes: u64) -> Self {
        Self {
            cache_dir,
            max_bytes,
            current_bytes: 0,
        }
    }

    pub fn cache_path(&self, asset_id: &str) -> PathBuf {
        self.cache_dir.join(asset_id)
    }

    pub fn has(&self, asset_id: &str) -> bool {
        self.cache_path(asset_id).exists()
    }

    pub fn store(&mut self, asset_id: &str, data: &[u8]) -> Result<(), crate::Error> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        // Enforce max_bytes: evict oldest entries if over budget
        let data_len = data.len() as u64;
        if self.current_bytes + data_len > self.max_bytes {
            self.evict_oldest_until_fit(data_len)?;
        }

        std::fs::write(self.cache_path(asset_id), data)
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
        self.current_bytes += data_len;
        Ok(())
    }

    fn evict_oldest_until_fit(&mut self, incoming_bytes: u64) -> Result<(), crate::Error> {
        let target = self.max_bytes.saturating_sub(incoming_bytes);
        let mut entries: Vec<(String, u64, std::time::SystemTime)> = Vec::new();

        if let Ok(rd) = std::fs::read_dir(&self.cache_dir) {
            for entry in rd.flatten() {
                if let Ok(meta) = entry.metadata() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        entries.push((name.to_string(), meta.len(), mtime));
                    }
                }
            }
        }

        // Sort by mtime ascending (oldest first)
        entries.sort_by_key(|a| a.2);

        for (name, size, _) in &entries {
            if self.current_bytes <= target {
                break;
            }
            let path = self.cache_dir.join(name);
            let _ = std::fs::remove_file(&path);
            self.current_bytes = self.current_bytes.saturating_sub(*size);
        }

        Ok(())
    }

    pub fn load(&self, asset_id: &str) -> Result<Vec<u8>, crate::Error> {
        std::fs::read(self.cache_path(asset_id))
            .map_err(|e| crate::Error::Storage(e.to_string().into()))
    }

    pub fn evict(&mut self, asset_id: &str) -> Result<(), crate::Error> {
        let path = self.cache_path(asset_id);
        if path.exists() {
            let meta = std::fs::metadata(&path)
                .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
            std::fs::remove_file(&path).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
            self.current_bytes = self.current_bytes.saturating_sub(meta.len());
        }
        Ok(())
    }
}
