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
//!
//! A host page can also play the scenario library through four optional
//! methods, JSON strings in and out (COMPATIBILITY.md names them as the
//! module's contract):
//!
//! ```js
//! const cases = JSON.parse(handle.scenarios());   // works before start()
//! handle.start_scenario("pcs-trip-and-reset");    // throws with a message
//! handle.load_scenario_yaml(text);                // throws with the position
//! const status = JSON.parse(handle.scenario_status());
//! ```
//!
//! The status has the shape `GET /api/v1/scenario` publishes, built by the
//! same function. The page cannot read plant data: the scene and the
//! panels stay the only readers of the state tree.

#[cfg(target_arch = "wasm32")]
mod web {
    use bess_scene::viewer::{library, Session, ViewerApp};
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

        /// The embedded scenario library: a JSON array of `{name, taxonomy,
        /// length_s, headline}`. Works before `start`.
        pub fn scenarios(&self) -> String {
            library::catalogue_json()
        }

        /// Throw the plant away and play the library case `name`, as the
        /// viewer's own picker does. Throws with a message naming the
        /// library's cases if there is no such case.
        pub fn start_scenario(&self, name: &str) -> Result<(), JsValue> {
            self.with_session(|s| s.start_scenario(name))?
                .map_err(|e| JsValue::from_str(&e))
        }

        /// Validate a scenario file's text against the site and play it.
        /// Throws with the refusal and its position.
        pub fn load_scenario_yaml(&self, text: &str) -> Result<(), JsValue> {
            self.with_session(|s| s.load_scenario_yaml(text))?
                .map_err(|e| JsValue::from_str(&e))
        }

        /// The case's status at the current tick, as a JSON string in the
        /// shape of `GET /api/v1/scenario`; `{"loaded":false}` when none
        /// plays.
        pub fn scenario_status(&self) -> Result<String, JsValue> {
            self.with_session(|s| s.scenario_status_json())
        }
    }

    impl WebHandle {
        /// Run `f` on the viewer's session between frames.
        fn with_session<T>(&self, f: impl FnOnce(&mut Session) -> T) -> Result<T, JsValue> {
            let mut app = self.runner.app_mut::<ViewerApp>().ok_or_else(|| {
                JsValue::from_str("the viewer is not running: await start() first")
            })?;
            Ok(f(app.session_mut()))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::WebHandle;
