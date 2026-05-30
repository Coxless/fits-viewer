//! Browser-side FITS viewer entry point (Wasm target).
//!
//! Build with:
//!   cd crates/fitsview-wasm && trunk build --release
//!
//! Wasm constraints vs native:
//! - `memmap2` is `#[cfg(not(target_arch = "wasm32"))]` gated in fitsview-core
//! - Large-file tile loading uses XHR Range requests instead of OS mmap

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() {
    use eframe::WebOptions;

    console_error_panic_hook::set_once();

    let web_options = WebOptions {
        follow_system_theme: true,
        ..Default::default()
    };

    eframe::WebRunner::new()
        .start(
            "fits_viewer_canvas",
            web_options,
            Box::new(|cc| {
                Ok(Box::new(fitsview_gui::app::FitsViewApp::new(
                    cc,
                    vec![],
                    None,
                )))
            }),
        )
        .await
        .expect("failed to start eframe WebRunner");
}

/// Fetch a byte range from a URL using XHR Range requests.
///
/// This is the Wasm replacement for `memmap2`-based tile loading.
/// Native builds use the OS mmap path in `fitsview-core::mmap_reader`.
#[cfg(target_arch = "wasm32")]
pub mod xhr_reader {
    use js_sys::Uint8Array;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{XmlHttpRequest, XmlHttpRequestResponseType};

    /// Fetch bytes `[start, start + len)` from `url` using an HTTP Range request.
    pub async fn fetch_range(url: &str, start: u64, len: u64) -> anyhow::Result<Vec<u8>> {
        let xhr = XmlHttpRequest::new().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        xhr.open("GET", url).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        xhr.set_response_type(XmlHttpRequestResponseType::Arraybuffer);
        xhr.set_request_header(
            "Range",
            &format!("bytes={start}-{}", start + len - 1),
        ).map_err(|e| anyhow::anyhow!("{e:?}"))?;

        // Wrap send + onload in a Promise
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let xhr_c = xhr.clone();
            let onload = Closure::once_into_js(move || {
                let _ = resolve.call1(&JsValue::NULL, &xhr_c.response().unwrap_or(JsValue::NULL));
            });
            let onerror = Closure::once_into_js(move || {
                let _ = reject.call0(&JsValue::NULL);
            });
            xhr.set_onload(Some(onload.as_ref().unchecked_ref()));
            xhr.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            xhr.send().unwrap_or_default();
        });

        let result = JsFuture::from(promise).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let array = Uint8Array::new(&result);
        Ok(array.to_vec())
    }
}
