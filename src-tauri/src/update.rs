/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

//! Desktop auto-update. Release builds read the signed manifest of the latest
//! GitHub release at startup and, if the user agrees, install the newer version
//! and restart into it.

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

/// Checks for an update in the background. Failed checks, such as starting
/// offline, are only logged.
pub fn spawn_check(app: AppHandle) {
  tauri::async_runtime::spawn(async move {
    if let Err(err) = check(&app).await {
      log::warn!("update: {err}");
    }
  });
}

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
  let Some(update) = app.updater()?.check().await? else {
    return Ok(());
  };
  log::info!("update: {} is available", update.version);

  let install = app
    .dialog()
    .message(format!(
      "Verditum {} sürümü yayımlandı. Şimdi yüklensin mi? Uygulama, yükleme \
       bitince yeniden başlatılacak.",
      update.version
    ))
    .title("Güncelleme")
    .buttons(MessageDialogButtons::OkCancelCustom(
      "Yükle".to_owned(),
      "Sonra".to_owned(),
    ))
    .blocking_show();
  if !install {
    return Ok(());
  }

  // Windows hands over to the installer, which exits and relaunches the app.
  if let Err(err) = update.download_and_install(|_, _| {}, || {}).await {
    app
      .dialog()
      .message(
        "Güncelleme yüklenemedi. Uygulama bir sonraki açılışta yeniden \
         deneyecek.",
      )
      .title("Güncelleme")
      .kind(MessageDialogKind::Error)
      .blocking_show();
    return Err(err);
  }
  app.restart();
}
