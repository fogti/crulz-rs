#![cfg(test)]

use super::*;

#[test]
fn test_args2unspaced() {
    use ASTNode::*;
    assert_eq!(
        CmdEvalArgs::from_wsdelim(vec![
            Constant {
                non_space: true,
                data: b"a".to_vec().into()
            },
            Constant {
                non_space: false,
                data: b"a".to_vec().into()
            },
            Constant {
                non_space: true,
                data: b"a".to_vec().into()
            },
            Constant {
                non_space: true,
                data: b"a".to_vec().into()
            },
            Constant {
                non_space: false,
                data: b"a".to_vec().into()
            }
        ]),
        CmdEvalArgs(vec![
            Constant {
                non_space: true,
                data: b"a".to_vec().into()
            },
            Constant {
                non_space: true,
                data: b"aa".to_vec().into()
            }
        ])
    );
}

#[test]
fn test_simplify() {
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
    assert_eq!(
        ast.simplify(),
        Constant {
            non_space: true,
            data: b"abc".to_vec().into()
        }
    );
}

#[test]
fn test_compact_tl() {
    let ast = vec![
        Constant {
            non_space: true,
            data: b"a".to_vec().into(),
        },
        Constant {
            non_space: false,
            data: b"b".to_vec().into(),
        },
        Constant {
            non_space: true,
            data: b"c".to_vec().into(),
        },
    ]
    .compact_toplevel();
    assert_eq!(
        ast,
        vec![Constant {
            non_space: true,
            data: b"abc".to_vec().into()
        }]
    );
}
