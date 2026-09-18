/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

mod pdf;
mod signature;
mod udf;
#[cfg(desktop)]
mod update;

use serde::Serialize;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};
use tauri::{Emitter, Manager, Url};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
#[cfg(desktop)]
use tauri_plugin_opener::OpenerExt;
#[cfg(mobile)]
use tauri_plugin_view::{ViewExt, ViewRequest};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenPaylod {
  content: String,
  size: usize,
  file_path: FilePath,
  signature: signature::Report,
}

fn open(app: &tauri::AppHandle, file_path: FilePath) {
  let mut options = OpenOptions::new();
  options.read(true);

  let result = app
    .fs()
    .open(file_path.clone(), options)
    .map_err(|_| "Dosya açılamadı.")
    .and_then(udf::read);
  match result {
    Ok(document) => {
      if let Ok(mut state) = app.state::<Mutex<AppState>>().lock() {
        state.file_path = Some(file_path.clone());
      }
      if let Err(err) = app.emit_to(
        "main",
        "add-content",
        OpenPaylod {
          size: document.content.len(),
          content: document.content,
          file_path,
          signature: document.signature,
        },
      ) {
        log::error!("add-content: {err}");
      }
    }
    Err(message) => {
      if let Ok(mut state) = app.state::<Mutex<AppState>>().lock() {
        state.file_path = None;
      }
      if let Err(err) = app.emit_to("main", "open-error", message) {
        log::error!("open-error: {err}");
      }
    }
  }
}

#[tauri::command]
async fn pick(app: tauri::AppHandle) {
  let file_path = app.dialog().file().blocking_pick_file();
  match file_path {
    Some(file_path) => open(app.app_handle(), file_path),
    None => return,
  };
}

/// Renders the displayed document as a PDF into the application cache and
/// hands it to the platform viewer: the default PDF application on desktop,
/// the share sheet on mobile. Returns the renderer's warnings, if any.
#[tauri::command]
async fn export(
  app: tauri::AppHandle,
  content: String,
) -> Result<Vec<String>, String> {
  let output = pdf::generate(&content)?;

  let source = app
    .state::<Mutex<AppState>>()
    .lock()
    .ok()
    .and_then(|state| state.file_path.clone());
  let stem = source
    .and_then(|path| path.into_path().ok())
    .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
    .unwrap_or_else(|| "document".to_owned());

  // A directory per export keeps the document's own name on the file, which
  // the viewer shows, without overwriting a copy that is still open.
  let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |elapsed| elapsed.as_nanos());
  let dir = app
    .path()
    .app_cache_dir()
    .map_err(|err| format!("No cache directory: {err}"))?
    .join("pdf")
    .join(nanos.to_string());
  fs::create_dir_all(&dir)
    .map_err(|err| format!("Unable to create {}: {err}", dir.display()))?;
  let path = dir.join(format!("{stem}.pdf"));
  fs::write(&path, output.bytes)
    .map_err(|err| format!("Unable to write {}: {err}", path.display()))?;

  let path = path.to_string_lossy().into_owned();
  #[cfg(desktop)]
  app
    .opener()
    .open_path(path, None::<&str>)
    .map_err(|err| format!("Unable to open the PDF: {err}"))?;
  #[cfg(mobile)]
  app
    .view()
    .view(ViewRequest { path: Some(path) })
    .map_err(|err| format!("Unable to open the PDF: {err}"))?;

  Ok(output.warnings)
}

#[tauri::command]
async fn ready(app: tauri::AppHandle) {
  // `open` locks the same state, so the guard is released before opening.
  let urls = match app.state::<Mutex<AppState>>().lock() {
    Ok(mut state) => {
      state.ready = true;
      state.urls.clone()
    }
    Err(_) => return,
  };
  urls
    .into_iter()
    .for_each(|url| open(app.app_handle(), FilePath::Url(url)));
}

#[derive(Default)]
struct AppState {
  urls: Vec<Url>,
  file_path: Option<FilePath>,
  ready: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(AppState::default()))
    .setup(|_app| {
      #[cfg(any(windows, target_os = "linux"))]
      {
        let mut urls = Vec::new();
        for arg in env::args().skip(1) {
          if let Ok(url) = Url::from_file_path(&arg) {
            urls.push(url);
          }
        }
        if let Ok(mut state) = _app.state::<Mutex<AppState>>().lock() {
          state.urls = urls;
        }
      }
      #[cfg(desktop)]
      {
        _app
          .handle()
          .plugin(tauri_plugin_updater::Builder::new().build())?;
        // Development builds are not installed, so there is nothing to replace.
        if !cfg!(debug_assertions) {
          update::spawn_check(_app.handle().clone());
        }
      }
      Ok(())
    })
    .plugin(
      tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .build(),
    )
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_view::init())
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_dialog::init())
    .on_page_load(|view, _event| {
      if view.label() == "main" {
        let app = view.app_handle();
        if let Ok(mut state) = app.state::<Mutex<AppState>>().lock() {
          if state.ready {
            state.urls = vec![];
            state.file_path = None;
            state.ready = false;
          }
        }
      }
    })
    .invoke_handler(tauri::generate_handler![ready, pick, export])
    .build(tauri::generate_context!())
    .expect("error while running Verditum")
    .run(|_app, _event| {
      #[cfg(target_os = "macos")]
      if let tauri::RunEvent::Opened { urls } = _event {
        if let Ok(mut state) = _app.state::<Mutex<AppState>>().lock() {
          state.urls = urls;
          if state.ready {
            tauri::async_runtime::spawn(ready(_app.to_owned()));
          }
        }
      }
    });
}
