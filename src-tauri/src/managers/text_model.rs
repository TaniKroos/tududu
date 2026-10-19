//! Text models: downloadable LLMs (GGUF) that Tududu runs in llama.cpp's
//! server for text tasks, such as translating selected text into English.
//!
//! Kept apart from the speech-model registry ([`super::model::ModelManager`])
//! so a text model can never be picked as a transcription model. Downloads
//! share the speech models' Hugging Face cache, retry/resume logic and
//! `model-download-*` events, so the UI shows progress the same way.

use super::model::{hf_cached_path, hf_download_with_retries, HfDownloadOutcome, ModelManager};
use anyhow::{anyhow, Result};
use hf_hub::api::tokio::CancellationToken;
use log::{error, info};
use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// A text model Tududu offers for download, pinned to an exact file.
pub struct TextModelSpec {
    /// `"{repo_id}/{filename}"`, the same id scheme as speech models.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub repo_id: &'static str,
    pub revision: &'static str,
    pub filename: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

/// The text models Tududu can download, best default first.
pub static TEXT_MODELS: &[TextModelSpec] = &[TextModelSpec {
    id: "ggml-org/gemma-3-1b-it-GGUF/gemma-3-1b-it-Q4_K_M.gguf",
    name: "Gemma 3 1B",
    description: "Small, fast model by Google for translating text into English.",
    repo_id: "ggml-org/gemma-3-1b-it-GGUF",
    revision: "f9c28bcd85737ffc5aef028638d3341d49869c27",
    filename: "gemma-3-1b-it-Q4_K_M.gguf",
    size_bytes: 806_058_240,
    sha256: "8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135",
}];

/// A text model as the UI sees it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct TextModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
}

pub struct TextModelManager {
    app_handle: AppHandle,
    /// Cancellation tokens of downloads in flight, by model id.
    downloads: Mutex<HashMap<String, CancellationToken>>,
}

impl TextModelManager {
    pub fn new(app_handle: &AppHandle) -> Self {
        Self {
            app_handle: app_handle.clone(),
            downloads: Mutex::new(HashMap::new()),
        }
    }

    pub fn spec(id: &str) -> Option<&'static TextModelSpec> {
        TEXT_MODELS.iter().find(|m| m.id == id)
    }

    fn cached_path(spec: &TextModelSpec) -> Option<PathBuf> {
        hf_cached_path(spec.repo_id, spec.revision, spec.filename)
    }

    pub fn list(&self) -> Vec<TextModelInfo> {
        let downloads = self.downloads.lock().unwrap();
        TEXT_MODELS
            .iter()
            .map(|spec| TextModelInfo {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                size_mb: spec.size_bytes / (1024 * 1024),
                is_downloaded: Self::cached_path(spec).is_some(),
                is_downloading: downloads.contains_key(spec.id),
            })
            .collect()
    }

    /// The model file on disk, if it's been downloaded.
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        let spec = Self::spec(id).ok_or_else(|| anyhow!("Unknown text model: {}", id))?;
        Self::cached_path(spec).ok_or_else(|| anyhow!("{} isn't downloaded", spec.name))
    }

    /// Download a text model into the shared Hugging Face cache and verify it.
    /// Returns once it's done, cancelled or failed; emits the same
    /// `model-download-*` events as speech models.
    pub async fn download(&self, id: &str) -> Result<()> {
        let spec = Self::spec(id).ok_or_else(|| anyhow!("Unknown text model: {}", id))?;
        if Self::cached_path(spec).is_some() {
            let _ = self.app_handle.emit("model-download-complete", spec.id);
            return Ok(());
        }
        let token = CancellationToken::new();
        {
            let mut downloads = self.downloads.lock().unwrap();
            if downloads.contains_key(spec.id) {
                return Err(anyhow!("{} is already downloading", spec.name));
            }
            downloads.insert(spec.id.to_string(), token.clone());
        }
        info!("Downloading text model {} ({})", spec.id, spec.revision);
        let outcome = hf_download_with_retries(
            &self.app_handle,
            spec.id,
            spec.repo_id,
            spec.revision,
            spec.filename,
            &token,
        )
        .await;
        self.downloads.lock().unwrap().remove(spec.id);

        let result = match outcome {
            Ok(HfDownloadOutcome::Completed) => self.verify(spec),
            Ok(HfDownloadOutcome::Cancelled) => return Ok(()),
            Ok(HfDownloadOutcome::Failed { error, attempts }) => Err(anyhow!(
                "Download failed after {} attempt(s): {}",
                attempts,
                error
            )),
            Err(e) => Err(e),
        };
        match &result {
            Ok(()) => {
                info!("Text model {} downloaded", spec.id);
                let _ = self.app_handle.emit("model-download-complete", spec.id);
            }
            Err(e) => {
                error!("Text model {} download failed: {}", spec.id, e);
                let _ = self.app_handle.emit(
                    "model-download-failed",
                    serde_json::json!({ "model_id": spec.id, "error": e.to_string() }),
                );
            }
        }
        result
    }

    /// Check the downloaded file against its pinned sha256, removing it if it
    /// doesn't match so the next download starts over.
    fn verify(&self, spec: &TextModelSpec) -> Result<()> {
        let path = Self::cached_path(spec)
            .ok_or_else(|| anyhow!("{} finished downloading but isn't in the cache", spec.name))?;
        let _ = self.app_handle.emit("model-verification-started", spec.id);
        let digest = sha256_file(&path);
        let _ = self
            .app_handle
            .emit("model-verification-completed", spec.id);
        match digest {
            Ok(digest) if digest == spec.sha256 => Ok(()),
            Ok(digest) => {
                ModelManager::delete_hf_cache_file(spec.repo_id, spec.revision, spec.filename);
                Err(anyhow!(
                    "{} failed verification (sha256 {} instead of {}); it was removed, download it again",
                    spec.name,
                    digest,
                    spec.sha256
                ))
            }
            Err(e) => Err(anyhow!("Couldn't verify {}: {}", spec.name, e)),
        }
    }

    pub fn cancel(&self, id: &str) {
        if let Some(token) = self.downloads.lock().unwrap().get(id) {
            token.cancel();
            let _ = self.app_handle.emit("model-download-cancelled", id);
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let spec = Self::spec(id).ok_or_else(|| anyhow!("Unknown text model: {}", id))?;
        if !ModelManager::delete_hf_cache_file(spec.repo_id, spec.revision, spec.filename) {
            return Err(anyhow!("{} isn't downloaded", spec.name));
        }
        info!("Deleted text model {}", spec.id);
        Ok(())
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::TEXT_MODELS;

    #[test]
    fn text_model_ids_follow_the_repo_and_file_scheme() {
        for spec in TEXT_MODELS {
            assert_eq!(spec.id, format!("{}/{}", spec.repo_id, spec.filename));
            assert_eq!(spec.sha256.len(), 64, "{} sha256", spec.id);
            assert_eq!(spec.revision.len(), 40, "{} revision", spec.id);
        }
    }
}
