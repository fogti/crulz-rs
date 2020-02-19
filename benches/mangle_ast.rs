#![feature(test)]

use crulz::{
    ast::{ASTNode::*, LiftAST},
    mangle_ast::{MangleAST, MangleASTExt},
};
extern crate test;

#[bench]
fn bench_simplify(b: &mut test::Bencher) {
    let ast = vec![
        Constant(true, "a".into()),
        Constant(true, "b".into())
            .lift_ast()
            .lift_ast()
            .lift_ast()
            .lift_ast(),
        Constant(true, "c".into()),
    ]
    .lift_ast()
    .lift_ast()
    .lift_ast();
    b.iter(|| ast.clone().simplify());
}

#[bench]
fn bench_compact_tl(b: &mut test::Bencher) {
    let ast = vec![
        Constant(true, "a".into()),
        Constant(false, "b".into()).lift_ast().lift_ast(),
        Constant(true, "a".into()),
        Constant(false, "b".into()).lift_ast().lift_ast(),
        Constant(true, "c".into()),
    ]
    .simplify();
    b.iter(|| ast.clone().compact_toplevel());
}
