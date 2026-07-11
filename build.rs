// build.rs — compile .po files to .mo at build time so the binary can load
// translations from OUT_DIR during development. Production packaging installs
// the .mo files into the GNU-standard /usr/share/locale directory.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const DOMAIN: &str = "clipsneko-installer";
const LANGS: &[&str] = &["en", "zh_CN", "zh_TW", "ja", "de", "ko", "ru"];

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set");
    let po_root = Path::new(&manifest_dir).join("po");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is always set"));
    let locale_root = out_dir.join("locale");

    for lang in LANGS {
        let po_path = po_root
            .join(lang)
            .join("LC_MESSAGES")
            .join(format!("{DOMAIN}.po"));
        if !po_path.exists() {
            panic!("missing translation catalog: {}", po_path.display());
        }
        let mo_dir = locale_root.join(lang).join("LC_MESSAGES");
        std::fs::create_dir_all(&mo_dir).expect("failed to create LC_MESSAGES dir in OUT_DIR");
        let mo_path = mo_dir.join(format!("{DOMAIN}.mo"));
        let status = Command::new("msgfmt")
            .arg("--output")
            .arg(&mo_path)
            .arg(&po_path)
            .status()
            .expect("msgfmt not found; install gettext");
        if !status.success() {
            panic!("msgfmt failed for {lang}");
        }
        println!("cargo:rerun-if-changed={}", po_path.display());
    }

    // Expose the development locale dir to debug builds. Release builds use
    // /usr/share/locale directly (see src/i18n.rs).
    println!(
        "cargo:rustc-env=CLIPSNEKO_DEV_LOCALEDIR={}",
        locale_root.display()
    );
    println!("cargo:rerun-if-changed=po/clipsneko-installer.pot");
}
