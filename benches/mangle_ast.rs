#![feature(test)]

use crulz::{
    ast::{ASTNode::*, LiftAST},
    mangle_ast::{MangleAST, MangleASTExt},
};
extern crate test;

#[bench]
fn bench_simplify(b: &mut test::Bencher) {
    let ast = vec![
        Constant(b"a".to_vec().into()),
        Constant(b"b".to_vec().into())
            .lift_ast()
            .lift_ast()
            .lift_ast()
            .lift_ast(),
        Constant(b"c".to_vec().into()),
    ]
    .lift_ast()
    .lift_ast()
    .lift_ast();
    b.iter(|| ast.clone().simplify());
}

#[bench]
fn bench_compact_tl(b: &mut test::Bencher) {
    let ast = vec![
        Constant(b"a".to_vec().into()),
        Constant(b" ".to_vec().into()).lift_ast().lift_ast(),
        Constant(b"a".to_vec().into()),
        Constant(b" ".to_vec().into()).lift_ast().lift_ast(),
        Constant(b"c".to_vec().into()),
    ]
    .simplify();
    b.iter(|| ast.clone().compact_toplevel());
}
