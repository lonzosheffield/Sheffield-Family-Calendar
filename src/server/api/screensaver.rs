//! Screensaver server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T2.7** from
//! here on (phone uploads through T2.5's pipeline, idle timeout, optional
//! schedule).

use dioxus::prelude::*;

/// Web paths of the ambient screensaver photos, sorted by file name.
#[server(endpoint = "list_screensaver_images")]
pub async fn list_screensaver_images() -> Result<Vec<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        // The on-disk directory is resolved from `FamilyHubConfig`
        // (T0.5: absolute, under `FAMILY_HUB_DATA_DIR`), independent of the
        // URL route the screensaver is served under (`/assets/screensaver`,
        // wired up by T0.6's `ServeDir`).
        const URL_PREFIX: &str = "/assets/screensaver";

        let dir = crate::server::config::FamilyHubConfig::load().screensaver_dir();
        let mut images = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return Ok(images);
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_image = matches!(
                std::path::Path::new(&name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("jpg" | "jpeg" | "png" | "webp" | "avif")
            );
            if is_image {
                images.push(format!("{URL_PREFIX}/{name}"));
            }
        }

        images.sort();
        Ok(images)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}
