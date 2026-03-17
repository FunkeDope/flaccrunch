use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=android-patches/DialogPlugin.kt");
    println!("cargo:rerun-if-changed=android-patches/MainActivity.kt");
    println!("cargo:rerun-if-changed=android-patches/AndroidBridgePlugin.kt");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        if let Err(err) = prepare_android_build() {
            println!("cargo:warning=Android patch preparation failed: {err}");
        }
    }

    tauri_build::build()
}

fn prepare_android_build() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let patches_dir = manifest_dir.join("android-patches");

    patch_dialog_plugin(&patches_dir.join("DialogPlugin.kt"))?;
    patch_generated_android_project(&manifest_dir, &patches_dir)?;

    Ok(())
}

fn patch_dialog_plugin(source: &Path) -> Result<(), String> {
    let Some(registry_src) = cargo_registry_src_dir() else {
        return Ok(());
    };

    let entries = fs::read_dir(&registry_src)
        .map_err(|e| format!("Failed to read Cargo registry source dir {registry_src:?}: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("tauri-plugin-dialog-") {
            continue;
        }

        let target = path
            .join("android")
            .join("src")
            .join("main")
            .join("java")
            .join("DialogPlugin.kt");
        if target.exists() {
            copy_if_different(source, &target)?;
            println!(
                "cargo:warning=Patched Android dialog plugin at {}",
                target.display()
            );
            return Ok(());
        }
    }

    println!(
        "cargo:warning=Could not find tauri-plugin-dialog Android source under {}",
        registry_src.display()
    );
    Ok(())
}

fn patch_generated_android_project(manifest_dir: &Path, patches_dir: &Path) -> Result<(), String> {
    let app_dir = manifest_dir.join("gen").join("android").join("app");
    if !app_dir.exists() {
        println!(
            "cargo:warning=Android generated project not found at {}. Run `npx tauri android init` first.",
            app_dir.display()
        );
        return Ok(());
    }

    let main_activity = app_dir
        .join("src")
        .join("main")
        .join("java")
        .join("com")
        .join("flaccrunch")
        .join("app")
        .join("MainActivity.kt");
    if main_activity.exists() {
        copy_if_different(&patches_dir.join("MainActivity.kt"), &main_activity)?;
    }

    let bridge_dir = app_dir
        .join("src")
        .join("main")
        .join("java")
        .join("com")
        .join("flaccrunch")
        .join("bridge");
    fs::create_dir_all(&bridge_dir)
        .map_err(|e| format!("Failed to create bridge dir {}: {e}", bridge_dir.display()))?;
    copy_if_different(
        &patches_dir.join("AndroidBridgePlugin.kt"),
        &bridge_dir.join("AndroidBridgePlugin.kt"),
    )?;

    let manifest = app_dir.join("src").join("main").join("AndroidManifest.xml");
    if manifest.exists() {
        patch_manifest(&manifest)?;
    }

    Ok(())
}

fn patch_manifest(path: &Path) -> Result<(), String> {
    let original =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut updated = original.clone();

    if !updated.contains("android.permission.READ_MEDIA_AUDIO") {
        updated = updated.replace(
            "<application ",
            "<uses-permission android:name=\"android.permission.READ_MEDIA_AUDIO\" />\n    \
<uses-permission android:name=\"android.permission.READ_EXTERNAL_STORAGE\" android:maxSdkVersion=\"32\" />\n    \
<application ",
        );
    }

    if !updated.contains("android:requestLegacyExternalStorage=\"true\"") {
        updated = updated.replace(
            "<application ",
            "<application android:requestLegacyExternalStorage=\"true\" ",
        );
    }

    if updated != original {
        fs::write(path, updated).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }

    Ok(())
}

fn copy_if_different(source: &Path, target: &Path) -> Result<(), String> {
    let source_bytes =
        fs::read(source).map_err(|e| format!("Failed to read {}: {e}", source.display()))?;
    let should_write = match fs::read(target) {
        Ok(existing) => existing != source_bytes,
        Err(_) => true,
    };

    if should_write {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        fs::write(target, source_bytes)
            .map_err(|e| format!("Failed to write {}: {e}", target.display()))?;
    }

    Ok(())
}

fn cargo_registry_src_dir() -> Option<PathBuf> {
    if let Ok(cargo_home) = env::var("CARGO_HOME") {
        let path = PathBuf::from(cargo_home).join("registry").join("src");
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(user_profile) = env::var("USERPROFILE") {
        let path = PathBuf::from(user_profile)
            .join(".cargo")
            .join("registry")
            .join("src");
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(home) = env::var("HOME") {
        let path = PathBuf::from(home)
            .join(".cargo")
            .join("registry")
            .join("src");
        if path.exists() {
            return Some(path);
        }
    }

    None
}
