//! Downloads a pinned Prism release from GitHub and bundles the languages the
//! web view offers into `$OUT_DIR/prism.js`, which `main.rs` embeds.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

const PRISM_VERSION: &str = "1.30.0";
/// SHA-256 of the GitHub source tarball for `v{PRISM_VERSION}`.
const PRISM_SHA256: &str = "e66b60aa1ab4c471cc871d652e9a0b2d29d7ecacf23ff1f5bf34ff4ea0091196";
/// Prism languages to bundle, in load order: a language must come after the one it extends.
/// Each also needs an `<option>` in `src/index.html`.
const PRISM_LANGS: &[&str] = &[
    "markup",
    "clike",
    "javascript",
    "json",
    "log",
    "diff",
    "yaml",
    "bash",
    "python",
    "sql",
    "toml",
    "docker",
];
/// Path to an already downloaded tarball, for offline builds. It is still checked against `PRISM_SHA256`.
const TARBALL_ENV: &str = "SPIDERPIPE_PRISM_TARBALL";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed={TARBALL_ENV}");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let tarball = match env::var_os(TARBALL_ENV) {
        Some(path) => PathBuf::from(path),
        None => {
            let path = out.join(format!("prism-{PRISM_VERSION}.tar.gz"));
            if !path.exists() || sha256(&path) != PRISM_SHA256 {
                download(&path);
            }
            path
        }
    };
    let actual = sha256(&tarball);
    if actual != PRISM_SHA256 {
        fail(&format!(
            "{} has SHA-256 {actual}, expected {PRISM_SHA256}",
            tarball.display()
        ));
    }

    let root = format!("prism-{PRISM_VERSION}");
    let mut members = vec![
        format!("{root}/LICENSE"),
        format!("{root}/components/prism-core.min.js"),
    ];
    members.extend(
        PRISM_LANGS
            .iter()
            .map(|l| format!("{root}/components/prism-{l}.min.js")),
    );
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&out)
        .args(&members));

    let read = |member: &str| {
        let path = out.join(member);
        fs::read_to_string(&path).unwrap_or_else(|e| fail(&format!("read {}: {e}", path.display())))
    };
    let mut bundle = format!(
        "/* Prism {PRISM_VERSION} (https://prismjs.com), languages: {}\n\n",
        PRISM_LANGS.join(" ")
    );
    for line in read(&members[0]).lines() {
        bundle.push_str(&format!("   {line}\n"));
    }
    bundle.push_str("*/\n");
    for member in &members[1..] {
        bundle.push_str(&read(member));
        bundle.push('\n');
    }
    fs::write(out.join("prism.js"), bundle).unwrap();
}

fn download(dest: &Path) {
    let url = format!("https://github.com/PrismJS/prism/archive/refs/tags/v{PRISM_VERSION}.tar.gz");
    run(Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(dest)
        .arg(&url));
}

fn sha256(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| fail(&format!("read {}: {e}", path.display())));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn run(cmd: &mut Command) {
    let name = cmd.get_program().to_string_lossy().into_owned();
    match cmd.status() {
        Ok(status) if status.success() => {}
        Ok(status) => fail(&format!("`{name}` exited with {status}")),
        Err(e) => fail(&format!("could not run `{name}`: {e}")),
    }
}

fn fail(msg: &str) -> ! {
    panic!(
        "fetching Prism {PRISM_VERSION} failed: {msg}\n\
         The build downloads Prism from GitHub with curl and tar. To build offline, download\n\
         https://github.com/PrismJS/prism/archive/refs/tags/v{PRISM_VERSION}.tar.gz and set {TARBALL_ENV} to its path."
    )
}
