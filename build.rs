use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=static/script.js");
    println!("cargo:rerun-if-changed=static/style.css");

    let out_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let static_dir = Path::new(&out_dir).join("static");

    let js_path = static_dir.join("script.js");
    let css_path = static_dir.join("style.css");

    let min_js_path = static_dir.join("script.min.js");
    let min_css_path = static_dir.join("style.min.css");

    if let Ok(js_code) = fs::read_to_string(js_path) {
        let minified_js = minifier::js::minify(&js_code);
        fs::write(min_js_path, minified_js.to_string()).expect("Failed to write minified JS");
    }

    if let Ok(css_code) = fs::read_to_string(css_path) {
        let minified_css = minifier::css::minify(&css_code).unwrap();
        fs::write(min_css_path, minified_css.to_string()).expect("Failed to write minified CSS");
    }
}
