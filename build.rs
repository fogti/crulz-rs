use std::env;
use std::path::Path;

fn main() {
    string_cache_codegen::AtomType::new("crulst::CrulzAtom", "crulst_atom!")
        .atoms(&[
            " ", "add", "def", "include", "pass", "suppress", "une", "unisp",
        ])
        .write_to_file(&Path::new(&env::var("OUT_DIR").unwrap()).join("crulst_atom.rs"))
        .unwrap()
}