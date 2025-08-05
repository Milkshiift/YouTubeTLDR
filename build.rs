use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// Minifies and compresses js, css and html

fn main() {
    println!("cargo:rerun-if-changed=static/script.js");
    println!("cargo:rerun-if-changed=static/style.css");
    println!("cargo:rerun-if-changed=static/index.html");

    let out_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let static_dir = Path::new(&out_dir).join("static");

    // --- Process JavaScript ---
    let js_path = static_dir.join("script.js");
    let br_js_path = static_dir.join("script.js.br");

    if let Ok(js_code) = fs::read_to_string(&js_path) {
        let minified_js = minifier::js::minify(&js_code);
        
        compress_with_brotli(&minified_js.to_string().as_bytes(), &br_js_path)
            .expect("Failed to compress minified JS with Brotli");
    } else {
        eprintln!("Warning: Could not read {}", js_path.display());
    }

    // --- Process CSS ---
    let css_path = static_dir.join("style.css");
    let br_css_path = static_dir.join("style.css.br");

    if let Ok(css_code) = fs::read_to_string(&css_path) {
        let minified_css = minifier::css::minify(&css_code)
            .expect("Failed to minify CSS");
        
        compress_with_brotli(&minified_css.to_string().as_bytes(), &br_css_path)
            .expect("Failed to compress minified CSS with Brotli");
    } else {
        eprintln!("Warning: Could not read {}", css_path.display());
    }

    // --- Process HTML ---
    let html_path = static_dir.join("index.html");
    let br_html_path = static_dir.join("index.html.br");

    if let Ok(html_content) = fs::read(&html_path) {
        compress_with_brotli(&html_content, &br_html_path)
            .expect("Failed to compress index.html with Brotli");
    } else {
        eprintln!("Warning: Could not read {}", html_path.display());
    }
}

fn compress_with_brotli(data: &[u8], output_path: &Path) -> io::Result<()> {
    let mut compressed_data = Vec::new();
    let mut writer = brotli::CompressorWriter::new(&mut compressed_data, 4096, 11, 22);

    writer.write_all(data)?;
    writer.flush()?;
    drop(writer);

    fs::write(output_path, &compressed_data)?;
    Ok(())
}