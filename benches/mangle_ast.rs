#![feature(test)]

use crulz::{
    ast::{ASTNode::*, LiftAST},
    mangle_ast::{MangleAST, MangleASTExt},
};
extern crate test;

#[bench]
fn bench_simplify(b: &mut test::Bencher) {
    let ast = vec![
        Constant(true, b"a".to_vec()),
        Constant(true, b"b".to_vec())
            .lift_ast()
            .lift_ast()
            .lift_ast()
            .lift_ast(),
        Constant(true, b"c".to_vec()),
    ]
    .lift_ast()
    .lift_ast()
    .lift_ast();
    b.iter(|| ast.clone().simplify());
}

#[bench]
fn bench_compact_tl(b: &mut test::Bencher) {
    let ast = vec![
        Constant(true, b"a".to_vec()),
        Constant(false, b"b".to_vec()).lift_ast().lift_ast(),
        Constant(true, b"a".to_vec()),
        Constant(false, b"b".to_vec()).lift_ast().lift_ast(),
        Constant(true, b"c".to_vec()),
    ]
    .simplify();
    b.iter(|| ast.clone().compact_toplevel());
}
