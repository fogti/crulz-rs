#![feature(test)]

use crulz::ast::{Lift as _, Mangle as _, MangleExt as _, Node::*};
extern crate test;

#[bench]
fn bench_simplify(b: &mut test::Bencher) {
    let ast = vec![
        Constant {
            non_space: true,
            data: b"a".to_vec().into(),
        },
        Constant {
            non_space: true,
            data: b"b".to_vec().into(),
        }
        .lift_ast()
        .lift_ast()
        .lift_ast()
        .lift_ast(),
        Constant {
            non_space: true,
            data: b"c".to_vec().into(),
        },
    ]
    .lift_ast()
    .lift_ast()
    .lift_ast();
    b.iter(|| ast.clone().simplify());
}

#[bench]
fn bench_compact_tl(b: &mut test::Bencher) {
    let ast = vec![
        Constant {
            non_space: true,
            data: b"a".to_vec().into(),
        },
        Constant {
            non_space: false,
            data: b"b".to_vec().into(),
        }
        .lift_ast()
        .lift_ast(),
        Constant {
            non_space: true,
            data: b"a".to_vec().into(),
        },
        Constant {
            non_space: false,
            data: b"b".to_vec().into(),
        }
        .lift_ast()
        .lift_ast(),
        Constant {
            non_space: true,
            data: b"c".to_vec().into(),
        },
    ]
    .simplify();
    b.iter(|| ast.clone().compact_toplevel());
}
