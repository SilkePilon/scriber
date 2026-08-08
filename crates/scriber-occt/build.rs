use std::{
    env,
    path::{Path, PathBuf},
};

/// Prefixes whose library directories the dynamic loader already searches, so
/// linking against them must NOT bake a RUNPATH into the binary.
///
/// `/usr` and `/usr/local` are on the default path of every glibc system.
/// `/app` is Flatpak's prefix: its runtime already puts `/app/lib` on the
/// loader path, and an absolute build-time rpath has no business inside a
/// relocatable bundle. Anything else is a custom prefix that the loader would
/// not find on its own.
const LOADER_DEFAULT_PREFIXES: &[&str] = &["/usr", "/usr/local", "/app"];

/// OCCT toolkits required by the current shim. Add to this list only when a new
/// shim function needs symbols from a toolkit that is not already here.
///
/// Verified against the OCCT 8.0.1 source tree:
///   TKernel, TKMath              — FoundationClasses
///   TKG2d, TKG3d, TKGeomBase,
///   TKBRep                       — ModelingData (GProp lives in TKGeomBase)
///   TKGeomAlgo, TKTopAlgo,
///   TKPrim, TKBO, TKShHealing,
///   TKMesh                       — ModelingAlgorithms (BRepGProp is in
///                                  TKTopAlgo, BRepPrimAPI in TKPrim,
///                                  BRepAlgoAPI in TKBO,
///                                  BRepMesh_IncrementalMesh in TKMesh)
///   TKXSBase, TKDESTEP,
///   TKDESTL                      — DataExchange (STEPControl is in TKDESTEP,
///                                  StlAPI_Writer in TKDESTL — note the name:
///                                  OCCT 7.7 renamed the old TKSTL, and both
///                                  7.9.x and 8.0.1 use TKDESTL)
const OCCT_TOOLKITS: &[&str] = &[
    "TKernel",
    "TKMath",
    "TKG2d",
    "TKG3d",
    "TKGeomBase",
    "TKBRep",
    "TKGeomAlgo",
    "TKTopAlgo",
    "TKPrim",
    "TKBO",
    "TKShHealing",
    "TKMesh",
    "TKXSBase",
    "TKDESTEP",
    "TKDESTL",
];

fn main() {
    let root = PathBuf::from(env::var("OCCT_ROOT").unwrap_or_else(|_| "/usr".into()));
    let include = root.join("include/opencascade");

    assert!(
        include.join("TopoDS_Shape.hxx").exists(),
        "OCCT headers not found at {}. Install opencascade-devel, or set \
         OCCT_ROOT to a prefix containing include/opencascade.",
        include.display()
    );

    cxx_build::bridge("src/lib.rs")
        .file("src/shim.cpp")
        .include(&include)
        .std("c++17")
        .compile("scriber-occt-shim");

    // rustc-link-search is a LINK-time path only. When OCCT lives somewhere the
    // dynamic loader does not already search, the build succeeds but every
    // resulting binary dies at startup on `libTKernel.so: cannot open shared
    // object file`. Emitting a RUNPATH alongside the search path is what makes
    // a custom prefix actually runnable.
    //
    // SCOPE LIMIT, verified rather than assumed: `cargo:rustc-link-arg` applies
    // only to the targets of the package emitting it, and does NOT propagate to
    // dependents the way rustc-link-search and rustc-link-lib do. What follows
    // therefore reaches this crate's own test binary and nothing else. TWINS:
    // `crates/scriber-kernel/build.rs` and `crates/scriber-cli/build.rs` repeat
    // the prefix decision below for exactly that reason — without them
    // `scriber`, scriber-kernel's test binary and the CLI's test binary come out
    // bare and die at startup under a custom OCCT_ROOT. Change the rule here and
    // you must change it in both twins.
    let needs_rpath = !LOADER_DEFAULT_PREFIXES
        .iter()
        .any(|prefix| root == Path::new(prefix));

    for dir in ["lib64", "lib"] {
        let candidate = root.join(dir);
        if candidate.exists() {
            println!("cargo:rustc-link-search=native={}", candidate.display());

            if needs_rpath {
                // --disable-new-dtags is load-bearing: it emits DT_RPATH rather
                // than DT_RUNPATH. The linker's --as-needed leaves most OCCT
                // toolkits out of the binary's own DT_NEEDED (they arrive
                // transitively through the six that survive), and DT_RUNPATH is
                // NOT consulted for a dependency's own dependencies while
                // DT_RPATH is inherited down the chain. With DT_RUNPATH the
                // loader finds libTKernel and then dies on libTKG2d.
                println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", candidate.display());
            }
        }
    }

    // dylib, never static: the Open CASCADE LGPL exception covers header
    // material only, so the library itself must remain dynamically linked.
    for toolkit in OCCT_TOOLKITS {
        println!("cargo:rustc-link-lib=dylib={toolkit}");
    }

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/shim.cpp");
    println!("cargo:rerun-if-changed=src/shim.hpp");
    println!("cargo:rerun-if-env-changed=OCCT_ROOT");
}
