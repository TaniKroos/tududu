//! Commands for text models (download, delete) and text translation.

use crate::managers::text_engine::TextEngine;
use crate::managers::text_model::{TextModelInfo, TextModelManager};
use crate::settings::get_settings;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

/// Translate `text` into English with the selected text model.
///
/// Tasks route by input: speech goes to the speech model (see
/// `managers::transcription`); text goes here, to the text model running in
/// the text engine.
pub(crate) async fn translate_text_to_english(
    app: &AppHandle,
    text: &str,
) -> anyhow::Result<String> {
    let model_id = get_settings(app).selected_text_model;
    let model = app.state::<TextModelManager>().path(&model_id)?;
    let engine = app.state::<Arc<TextEngine>>().inner().clone();
    engine.translate_to_english(&model, text).await
}

#[tauri::command]
#[specta::specta]
pub fn get_text_models(manager: State<'_, TextModelManager>) -> Vec<TextModelInfo> {
    manager.list()
}

/// Download a text model; resolves when the download finishes.
#[tauri::command]
#[specta::specta]
pub async fn download_text_model(app: AppHandle, model_id: String) -> Result<(), String> {
    app.state::<TextModelManager>()
        .download(&model_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_text_model_download(manager: State<'_, TextModelManager>, model_id: String) {
    manager.cancel(&model_id);
}

#[tauri::command]
#[specta::specta]
pub async fn delete_text_model(app: AppHandle, model_id: String) -> Result<(), String> {
    // The engine may have the file open (on Windows that blocks deleting it).
    app.state::<Arc<TextEngine>>().stop().await;
    app.state::<TextModelManager>()
        .delete(&model_id)
        .map_err(|e| e.to_string())
}

/// Translate text into English, for the settings page's "try it" box.
#[tauri::command]
#[specta::specta]
pub async fn translate_text(app: AppHandle, text: String) -> Result<String, String> {
    translate_text_to_english(&app, &text)
        .await
        .map_err(|e| e.to_string())
}

/// Hide the overlay, e.g. when the user clicks a translation popup.
#[tauri::command]
#[specta::specta]
pub fn dismiss_overlay(app: AppHandle) {
    crate::utils::hide_recording_overlay(&app);
}
