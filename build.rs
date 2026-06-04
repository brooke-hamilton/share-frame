use std::env;
use std::fs;

/// Embeds the Windows icon, version resource, and side-by-side manifest.
///
/// All version metadata is single-sourced from `Cargo.toml`: `winresource`
/// derives the numeric `FILEVERSION`/`PRODUCTVERSION` and the version strings
/// from Cargo's `CARGO_PKG_VERSION*` variables, and we inject the same version
/// into the manifest's `assemblyIdentity` so the SxS identity can never drift.
fn main() {
    println!("cargo:rerun-if-changed=share-frame.manifest");
    println!("cargo:rerun-if-changed=assets/icons/share-frame.ico");

    // build.rs runs on the host, so guard on CARGO_CFG_TARGET_OS (the real
    // target) rather than #[cfg(target_os = ...)].
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // Inject the package version into the manifest's app assemblyIdentity. The
    // numeric VERSIONINFO is handled by winresource from CARGO_PKG_VERSION_*.
    let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
    let minor = env::var("CARGO_PKG_VERSION_MINOR").unwrap();
    let patch = env::var("CARGO_PKG_VERSION_PATCH").unwrap();
    let dotted = format!("{major}.{minor}.{patch}.0");

    let template =
        fs::read_to_string("share-frame.manifest").expect("failed to read share-frame.manifest");
    let needle = "version=\"0.1.0.0\"";
    assert_eq!(
        template.matches(needle).count(),
        1,
        "share-frame.manifest must contain exactly one `{needle}` placeholder for version injection",
    );
    let manifest = template.replace(needle, &format!("version=\"{dotted}\""));

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icons/share-frame.ico")
        .set_manifest(&manifest)
        .set("CompanyName", "Brooke Hamilton")
        .set("FileDescription", "Share Frame")
        .set("ProductName", "Share Frame")
        .set("InternalName", "share-frame")
        .set("OriginalFilename", "share-frame.exe");
    res.compile().expect("failed to compile Windows resources");
}
