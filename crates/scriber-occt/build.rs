use std::{env, path::PathBuf};

/// OCCT toolkits required by the current shim. Add to this list only when a new
/// shim function needs symbols from a toolkit that is not already here.
///
/// Verified against the OCCT 8.0.1 source tree:
///   TKernel, TKMath              — FoundationClasses
///   TKG2d, TKG3d, TKGeomBase,
///   TKBRep                       — ModelingData (GProp lives in TKGeomBase)
///   TKGeomAlgo, TKTopAlgo,
///   TKPrim, TKBO, TKShHealing    — ModelingAlgorithms (BRepGProp is in
///                                  TKTopAlgo, BRepPrimAPI in TKPrim,
///                                  BRepAlgoAPI in TKBO)
///   TKXSBase, TKDESTEP           — DataExchange (STEPControl is in TKDESTEP)
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
    "TKXSBase",
    "TKDESTEP",
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

    for dir in ["lib64", "lib"] {
        let candidate = root.join(dir);
        if candidate.exists() {
            println!("cargo:rustc-link-search=native={}", candidate.display());
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
