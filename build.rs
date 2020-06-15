use std::env;
use std::path::Path;

fn main() {
    // ignore changes to other files except build.rs
    println!("cargo:rerun-if-changed=build.rs");

    string_cache_codegen::AtomType::new("crulst::CrulzAtom", "crulst_atom!")
        .atoms(&[
            " ",
            "$",
            "{",
            "}",
            "add",
            "def",
            "def-lazy",
            "foreach",
            "foreach-raw",
            "fseq",
            "include",
            "pass",
            "suppress",
            "undef",
            "une",
            "unee",
        ])
        .write_to_file(&Path::new(&env::var("OUT_DIR").unwrap()).join("crulst_atom.rs"))
        .unwrap()
}
