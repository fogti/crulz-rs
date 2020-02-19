#![forbid(unsafe_code)]

#[macro_use]
#[allow(dead_code, clippy::unreadable_literal)]
mod crulst {
    include!(concat!(env!("OUT_DIR"), "/crulst_atom.rs"));
}

pub mod ast;
pub mod interp;
pub mod mangle_ast;
pub mod parser;
