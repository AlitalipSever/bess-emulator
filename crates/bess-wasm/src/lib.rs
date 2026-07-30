//! Browser entry: the deterministic kernel and the all-Rust view compiled
//! into one WASM module. No protocols, no JS data bridge; the scene and the
//! panels read the state tree directly from Rust memory.
//!
//! Build with `wasm-pack build crates/bess-wasm --target web` (ships as the
//! npm package) and start it from JS:
//!
//! ```js
//! import init, { WebHandle } from "bess-emulator";
//! await init();
//! const handle = new WebHandle();
//! await handle.start(document.getElementById("bess-canvas"));
//! ```

#[cfg(target_arch = "wasm32")]
mod web {
    use bess_scene::viewer::ViewerApp;
    use wasm_bindgen::prelude::*;

    /// Handle to a running emulator instance on a canvas.
    #[wasm_bindgen]
    pub struct WebHandle {
        runner: eframe::WebRunner,
    }

    #[wasm_bindgen]
    impl WebHandle {
        /// Prepare a runner. Call `start` to attach it to a canvas.
        #[wasm_bindgen(constructor)]
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            console_error_panic_hook::set_once();
            Self {
                runner: eframe::WebRunner::new(),
            }
        }

        /// Start the emulator on the given canvas. The simulation begins
        /// immediately at 60x.
        pub async fn start(
            &self,
            canvas: web_sys::HtmlCanvasElement,
        ) -> Result<(), wasm_bindgen::JsValue> {
            self.runner
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| {
                        ViewerApp::new(cc)
                            .map(|app| Box::new(app) as Box<dyn eframe::App>)
                            .map_err(Into::into)
                    }),
                )
                .await
        }

        /// Stop the emulator and free its resources.
        pub fn destroy(&self) {
            self.runner.destroy();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::WebHandle;
