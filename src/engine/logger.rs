use std::panic;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = info)]
    fn console_info(s: &str);

    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(s: &str);

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
}

pub fn init_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            &**s
        } else {
            "Box<dyn Any>"
        };

        let file = info.location().map(|l| l.file()).unwrap_or("<unknown>");
        let line = info.location().map(|l| l.line()).unwrap_or(0);
        let column = info.location().map(|l| l.column()).unwrap_or(0);

        let formatted = format!(
            "[Artisan Panic] at {}:{}:{}: \n\n{}",
            file, line, column, msg
        );
        console_error(&formatted);
    }));
}

#[wasm_bindgen]
pub fn wasm_info(msg: &str) {
    console_info(&format!("[Artisan] Info: {}", msg));
}

#[wasm_bindgen]
pub fn wasm_warn(msg: &str) {
    console_warn(&format!("[Artisan] Warning: {}", msg));
}

#[wasm_bindgen]
pub fn wasm_error(msg: &str) {
    console_error(&format!("[Artisan] Error: {}", msg));
}

#[macro_export]
macro_rules! log_info {
    ($($t:tt)*) => {
        $crate::wasm_info(&format!($($t)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($t:tt)*) => {
        $crate::wasm_warn(&format!($($t)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($t:tt)*) => {
        $crate::wasm_error(&format!($($t)*))
    };
}
