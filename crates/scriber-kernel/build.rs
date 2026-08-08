use std::{
    env,
    path::{Path, PathBuf},
};

/// Prefixes whose library directories the dynamic loader already searches, so
/// linking against them must NOT bake a RUNPATH into the binary.
///
/// TWIN: kept identical to `LOADER_DEFAULT_PREFIXES` in
/// `crates/scriber-occt/build.rs`; see `emit_occt_rpath` below for why the
/// decision has to be repeated here rather than shared.
const LOADER_DEFAULT_PREFIXES: &[&str] = &["/usr", "/usr/local", "/app"];

/// Bakes OCCT's library directory into this package's linked targets, but only
/// when OCCT sits outside a prefix the loader already searches.
///
/// TWIN of the same block in `crates/scriber-occt/build.rs` and
/// `crates/scriber-cli/build.rs`. It cannot be shared: `cargo:rustc-link-arg`
/// applies only to the targets of the package that emits it and does NOT
/// propagate to dependents the way `rustc-link-search` and `rustc-link-lib` do,
/// so scriber-occt's emission never reaches this package's test binary. Ten
/// lines of duplication cost less than a crate to hold them; keep the three
/// copies in step.
///
/// `cargo:rustc-link-arg` (no suffix) rather than `-tests`, deliberately: the
/// only target here that links is the unit-test binary built from `src/lib.rs`,
/// and cargo's `-tests` suffix matches `tests/*.rs` integration targets only, so
/// the suffixed form would emit nothing that lands anywhere.
fn emit_occt_rpath() {
    let root = PathBuf::from(env::var("OCCT_ROOT").unwrap_or_else(|_| "/usr".into()));

    if LOADER_DEFAULT_PREFIXES
        .iter()
        .any(|prefix| root == Path::new(prefix))
    {
        return;
    }

    for dir in ["lib64", "lib"] {
        let candidate = root.join(dir);
        if candidate.exists() {
            // --disable-new-dtags is load-bearing: it emits DT_RPATH rather than
            // DT_RUNPATH. --as-needed leaves most OCCT toolkits out of the
            // binary's own DT_NEEDED (they arrive transitively through the few
            // that survive), and DT_RUNPATH is NOT consulted for a dependency's
            // own dependencies while DT_RPATH is inherited down the chain.
            println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", candidate.display());
        }
    }
}

fn main() {
    emit_occt_rpath();

    println!("cargo:rerun-if-env-changed=OCCT_ROOT");
}
