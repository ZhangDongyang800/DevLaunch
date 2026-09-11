fn main() {
    // tauri-build 只跟踪 tauri.conf.json / capabilities；图标参与编译期嵌入，
    // 必须逐个文件显式声明，否则替换 icons/ 里的图标后 cargo 不会重编译。
    println!("cargo:rerun-if-changed=icons");
    if let Ok(entries) = std::fs::read_dir("icons") {
        for entry in entries.flatten() {
            let path = entry.path().to_string_lossy().replace('\\', "/");
            println!("cargo:rerun-if-changed={path}");
        }
    }
    tauri_build::build()
}
