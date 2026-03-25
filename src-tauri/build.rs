use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(embed_model)");
    println!("cargo::rustc-check-cfg=cfg(dev_mode)");

    tauri_build::build();

    // Model embedding for release builds
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let profile = env::var("PROFILE").unwrap_or_default();
    let is_release = profile == "release";

    if !is_release || env::var("VOICEINPUT_DEV").is_ok() {
        // Debug builds or explicit dev mode: don't embed model
        fs::write(out_dir.join("model_embedded"), "false").unwrap();
        println!("cargo:rustc-cfg=dev_mode");
        if !is_release {
            println!("cargo:warning=DEBUG BUILD: Model loaded from external path (use --release to embed)");
        } else {
            println!("cargo:warning=DEV MODE: Model will be loaded from external path");
        }
    } else {
        // Release mode: embed model into binary
        let model_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .unwrap()
            .join("ggml-small.bin");

        if model_path.exists() {
            // Compute SHA256 hash of the model file
            use sha2::{Digest, Sha256};
            let model_bytes = fs::read(&model_path).expect("Failed to read model file");
            let hash = Sha256::digest(&model_bytes);
            let hash_hex = hex::encode(hash);

            // Write hash for runtime verification
            fs::write(out_dir.join("model_hash"), &hash_hex).unwrap();
            fs::write(out_dir.join("model_embedded"), "true").unwrap();

            println!("cargo:rustc-cfg=embed_model");
            println!(
                "cargo:warning=Embedding model: {} ({} bytes, sha256: {})",
                model_path.display(),
                model_bytes.len(),
                &hash_hex[..16]
            );
            // The actual include_bytes! is done in the source code via cfg
        } else {
            fs::write(out_dir.join("model_embedded"), "false").unwrap();
            println!("cargo:rustc-cfg=dev_mode");
            println!(
                "cargo:warning=Model not found at {}, falling back to dev mode",
                model_path.display()
            );
        }
    }
}
