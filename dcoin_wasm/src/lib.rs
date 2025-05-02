use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn test(str: &str) -> String {
    format!("String + test = {}test", str)
}
