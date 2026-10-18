use crate::managers::model::{ModelInfo, ModelManager, RetiredModel};
use crate::managers::transcription::{ModelStateEvent, TranscriptionManager};
use crate::settings::{get_settings, write_settings, ModelUnloadTimeout};
use log::error;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
#[specta::specta]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

/// Re-scan local sources (custom models dir + shared HF cache) for models added
/// since launch
#[tauri::command]
#[specta::specta]
pub async fn rescan_local_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<(), String> {
    let mm = model_manager.inner().clone();
    tokio::task::spawn_blocking(move || mm.rescan_local_models())
        .await
        .map_err(|e| format!("rescan task panicked: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let result = model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string());

    if let Err(ref error) = result {
        // Log as well as emit: the toast is transient, and failed downloads have
        // historically been undiagnosable because logs showed nothing (#1579).
        error!("Model download failed for {}: {}", model_id, error);
        let _ = app_handle.emit(
            "model-download-failed",
            serde_json::json!({ "model_id": &model_id, "error": error }),
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub fn get_retired_models(model_manager: State<'_, Arc<ModelManager>>) -> Vec<RetiredModel> {
    model_manager.get_retired_models()
}

#[tauri::command]
#[specta::specta]
pub async fn delete_retired_model(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager.delete_retired_model(&model_id).map_err(|e| {
        error!("Retired model delete failed for {}: {}", model_id, e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // If deleting the active model, unload it and clear the setting
    let settings = get_settings(&app_handle);
    if settings.selected_model == model_id {
        // Waits for the worker to exit (behind any transcription in progress)
        // before the file is deleted, so keep it off the async workers.
        let tm = Arc::clone(&transcription_manager);
        tauri::async_runtime::spawn_blocking(move || tm.unload_model())
            .await
            .map_err(|e| format!("Failed to unload model: {}", e))?
            .map_err(|e| format!("Failed to unload model: {}", e))?;

        let mut settings = get_settings(&app_handle);
        settings.selected_model = String::new();
        write_settings(&app_handle, settings);
    }

    model_manager.delete_model(&model_id).map_err(|e| {
        // The UI only shows a generic failure toast, so the cause has to be logged.
        error!("Model delete failed for {}: {}", model_id, e);
        e.to_string()
    })
}

/// Shared logic for switching the active model, used by both the Tauri command
/// and the tray menu handler.
///
/// Validates the model, updates the persisted setting, and loads the model
/// unless the unload timeout is set to "Immediately" (in which case the model
/// will be loaded on-demand during the next transcription).
pub fn switch_active_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let model_manager = app.state::<Arc<ModelManager>>();
    let transcription_manager = app.state::<Arc<TranscriptionManager>>();

    // Atomically claim the loading slot — prevents concurrent model loads
    // from tray double-clicks or overlapping commands. The guard resets the
    // flag on drop (including early returns, errors, and panics).
    let _loading_guard = transcription_manager
        .try_start_loading()
        .ok_or_else(|| "Model load already in progress".to_string())?;

    // Check if model exists and is available
    let model_info = model_manager
        .get_model_info(model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }

    let settings = get_settings(app);
    let unload_timeout = settings.model_unload_timeout;
    let old_model = settings.selected_model.clone();
    let old_onboarding_completed = settings.onboarding_completed;

    // Persist the new selection early so the frontend sees the correct model
    // when it reacts to events emitted by load_model.
    let mut settings = settings;
    settings.selected_model = model_id.to_string();
    settings.onboarding_completed = true;

    write_settings(app, settings);

    // Skip eager loading if unload is set to "Immediately" — the model
    // will be loaded on-demand during the next transcription.
    if unload_timeout == ModelUnloadTimeout::Immediately {
        // Notify frontend — load_model won't be called so no events
        // would otherwise be emitted.
        let _ = app.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "selection_changed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );
        log::info!(
            "Model selection changed to {} (not loading — unload set to Immediately).",
            model_id
        );
        return Ok(());
    }

    // Load the model. On failure, revert the persisted selection.
    if let Err(e) = transcription_manager.load_model(model_id) {
        let mut settings = get_settings(app);
        settings.selected_model = old_model;
        settings.onboarding_completed = old_onboarding_completed;
        write_settings(app, settings);
        return Err(e.to_string());
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_model(
    app_handle: AppHandle,
    _model_manager: State<'_, Arc<ModelManager>>,
    _transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    switch_active_model(&app_handle, &model_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_model(app_handle: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app_handle);
    Ok(settings.selected_model)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_model_status(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Option<String>, String> {
    Ok(transcription_manager.get_current_model())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_loading(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<bool, String> {
    // Check if transcription manager has a loaded model
    let current_model = transcription_manager.get_current_model();
    Ok(current_model.is_none())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}

/// What the translate shortcut can do with a model.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TranslationOptions {
    /// Whether the model can translate at all.
    pub supported: bool,
    /// The language the model will treat speech as: the selected language,
    /// resolved for this model ("auto" when the model detects it).
    pub source_language: String,
    /// Languages speech in `source_language` can be translated into.
    pub targets: Vec<String>,
}

/// Translation targets for a model, read from the downloaded file's header
/// (catalog entries don't carry them) and narrowed to what the current source
/// language can reach.
#[tauri::command]
#[specta::specta]
pub async fn get_translation_options(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<TranslationOptions, String> {
    use crate::managers::model_capabilities::{CapabilityProber, GgufHeaderProber};

    let info = model_manager
        .get_model_info(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;
    let settings = get_settings(&app_handle);
    let source_language = crate::managers::model::effective_language(
        &settings.selected_language,
        &info.supported_languages,
        info.supports_language_detection,
    );
    let probe = model_manager
        .get_model_path(&model_id)
        .map(|path| GgufHeaderProber.probe_file(&path))
        .unwrap_or_default();
    let supported = probe
        .supports_translation
        .unwrap_or(info.supports_translation);
    let targets = reachable_translation_targets(
        supported,
        &source_language,
        probe.translation_targets.as_deref().unwrap_or_default(),
        probe.translation_pairs.as_deref().unwrap_or_default(),
        &info.supported_languages,
    );
    Ok(TranslationOptions {
        supported,
        source_language,
        targets,
    })
}

/// Languages a translate run from `source` can produce. Pairs (`src>dst`) are
/// the strictest information, then the advertised target list; a model that
/// advertises neither is assumed to translate between its own languages.
/// The source itself is never a target — that would just be a transcription.
fn reachable_translation_targets(
    supported: bool,
    source: &str,
    targets: &[String],
    pairs: &[String],
    languages: &[String],
) -> Vec<String> {
    use crate::managers::model::canonical_language_code as canon;

    if !supported {
        return Vec::new();
    }
    let is_source = |lang: &str| source != "auto" && canon(lang) == canon(source);
    let candidates: Vec<String> = if !pairs.is_empty() {
        pairs
            .iter()
            .filter_map(|pair| pair.split_once('>'))
            .filter(|(src, _)| source == "auto" || canon(src) == canon(source))
            .map(|(_, dst)| dst.to_string())
            .collect()
    } else if !targets.is_empty() {
        targets.to_vec()
    } else {
        languages.to_vec()
    };
    let mut out: Vec<String> = Vec::new();
    for lang in candidates {
        if !is_source(&lang) && !out.iter().any(|l| canon(l) == canon(&lang)) {
            out.push(lang);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::reachable_translation_targets;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pairs_limit_targets_to_what_the_source_reaches() {
        let pairs = v(&["en>de", "de>en", "en>fr", "fr>en"]);
        let targets = v(&["en", "de", "fr"]);

        assert_eq!(
            reachable_translation_targets(true, "en", &targets, &pairs, &[]),
            v(&["de", "fr"])
        );
        assert_eq!(
            reachable_translation_targets(true, "de", &targets, &pairs, &[]),
            v(&["en"])
        );
    }

    #[test]
    fn target_list_is_used_without_pairs_and_excludes_the_source() {
        let targets = v(&["en", "de", "fr"]);

        assert_eq!(
            reachable_translation_targets(true, "en", &targets, &[], &v(&["en", "lv"])),
            v(&["de", "fr"])
        );
    }

    #[test]
    fn models_that_cannot_translate_have_no_targets() {
        assert!(reachable_translation_targets(false, "en", &v(&["de"]), &[], &[]).is_empty());
    }

    #[test]
    fn auto_source_offers_every_target_once() {
        let pairs = v(&["en>de", "fr>de", "de>en"]);

        assert_eq!(
            reachable_translation_targets(true, "auto", &[], &pairs, &[]),
            v(&["de", "en"])
        );
    }
}
