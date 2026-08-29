#[cfg(target_arch = "wasm32")]
mod imp {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;

    struct Clock {
        perf: JsValue,
        now: js_sys::Function,

        origin: f64,
    }

    fn lookup() -> Option<Clock> {
        let global = js_sys::global();
        let perf = js_sys::Reflect::get(&global, &JsValue::from_str("performance")).ok()?;
        if perf.is_undefined() || perf.is_null() {
            return None;
        }
        let now = js_sys::Reflect::get(&perf, &JsValue::from_str("now"))
            .ok()?
            .dyn_into::<js_sys::Function>()
            .ok()?;

        let origin = js_sys::Reflect::get(&perf, &JsValue::from_str("timeOrigin"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        Some(Clock { perf, now, origin })
    }

    thread_local! {
        static PERF: Option<Clock> = lookup();
    }

    pub fn now_ms() -> f64 {
        PERF.with(|p| match p {
            Some(c) => {
                c.origin
                    + c.now
                        .call0(&c.perf)
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
            }
            None => 0.0,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::sync::OnceLock;
    use std::time::Instant;

    static ORIGIN: OnceLock<Instant> = OnceLock::new();

    pub fn now_ms() -> f64 {
        ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
    }
}

pub use imp::now_ms;
