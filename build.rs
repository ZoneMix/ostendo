use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("theme_list.rs");

    let themes_dir = Path::new("themes");
    let mut entries = Vec::new();

    if themes_dir.exists() {
        for entry in fs::read_dir(themes_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let rel = path.to_string_lossy().to_string();
                entries.push((stem, rel));
            }
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::from("pub const BUILTIN_THEMES: &[(&str, &str)] = &[\n");
    for (stem, rel) in &entries {
        // Use forward slashes for include_str! paths — works on all platforms
        // and avoids backslash escape interpretation on Windows
        let rel_fwd = rel.replace('\\', "/");
        code.push_str(&format!(
            "    (\"{}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\"))),\n",
            stem, rel_fwd
        ));
    }
    code.push_str("];\n");

    fs::write(dest_path, code).unwrap();
    println!("cargo:rerun-if-changed=themes");
}
