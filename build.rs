use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/schema-compare.ico");
    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
    let minor = env::var("CARGO_PKG_VERSION_MINOR").unwrap();
    let patch = env::var("CARGO_PKG_VERSION_PATCH").unwrap();
    fs::write(output.join("app-version.h"),format!("#define APP_VERSION_MAJOR {major}\n#define APP_VERSION_MINOR {minor}\n#define APP_VERSION_PATCH {patch}\n#define APP_VERSION_STRING \"{version}\\0\"\n")).expect("Writing application version resource failed");
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let resource = output.join(if gnu { "app-icon.o" } else { "app-icon.res" });
    let mut command = if gnu {
        let bundled = root.join(".tools/mingw64/bin/windres.exe");
        let mut cmd = Command::new(if bundled.exists() {
            bundled
        } else {
            PathBuf::from("windres")
        });
        cmd.args(["-i", "resources/app.rc", "--output-format=coff", "-o"])
            .arg(&resource)
            .arg("-I")
            .arg(&output);
        cmd
    } else {
        let mut cmd = Command::new("rc.exe");
        cmd.arg("/nologo")
            .arg(format!("/I{}", output.display()))
            .arg(format!("/fo{}", resource.display()))
            .arg("resources/app.rc");
        cmd
    };
    let status = command
        .current_dir(&root)
        .status()
        .expect("Windows resource compiler unavailable (windres for GNU / rc.exe for MSVC)");
    assert!(status.success(), "Compiling the application icon failed");
    for binary in ["schema-compare", "schema-compare-cli"] {
        println!("cargo:rustc-link-arg-bin={binary}={}", resource.display());
    }
}
