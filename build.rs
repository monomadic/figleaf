fn main() {
    println!("cargo:rerun-if-changed=src/text/coretext.c");
    if std::env::var_os("CARGO_FEATURE_TEXT_CORETEXT").is_none() {
        return;
    }
    assert_eq!(
        std::env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "macos",
        "text-coretext requires macOS"
    );
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let ok = std::process::Command::new("clang")
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-c",
            "src/text/coretext.c",
            "-o",
        ])
        .arg(out.join("coretext.o"))
        .status()
        .unwrap();
    assert!(ok.success());
    let ok = std::process::Command::new("ar")
        .arg("crs")
        .arg(out.join("libfigleaf_ct.a"))
        .arg(out.join("coretext.o"))
        .status()
        .unwrap();
    assert!(ok.success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=figleaf_ct");
    for f in ["CoreText", "CoreGraphics", "CoreFoundation"] {
        println!("cargo:rustc-link-lib=framework={f}");
    }
}
