use crate::ast::{ASTNode, Atom, VAN};
use crate::mangle_ast::MangleAST;
use crate::parser::ParserOptions;
use anyhow::Context;
use hashbrown::hash_map::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone)]
enum BuiltInFn {
    Manual(fn(&mut VAN, &mut EvalContext) -> Option<ASTNode>),
    Automatic(fn(VAN) -> Option<ASTNode>),
}

#[derive(Clone)]
enum InterpValue {
    BuiltIn(Option<usize>, BuiltInFn),
    Data(usize, ASTNode),
}

type DefinesMap = HashMap<Atom, InterpValue>;
type CompilatesMap<'a> = HashMap<&'a str, &'a str>;

struct EvalContext<'a> {
    defs: DefinesMap,
    opts: ParserOptions,
    comp_map: &'a CompilatesMap<'a>,
}

type CompiledDefinesMap = HashMap<String, (usize, ASTNode)>;

#[derive(Deserialize, Serialize)]
struct CompilateData {
    content: VAN,
    defs: CompiledDefinesMap,
}

impl EvalContext<'_> {
    pub fn load_from_compfile(&mut self, compf: &str) -> Result<VAN, anyhow::Error> {
        let fh = readfilez::read_from_file(std::fs::File::open(compf))
            .with_context(|| format!("Unable to open compfile '{}'", compf))?;
        let z = flate2::read::DeflateDecoder::new(fh.as_slice());
        let CompilateData {
            content,
            defs: ins_defs,
        } = bincode::deserialize_from(z)
            .with_context(|| format!("Unable to read compfile '{}'", compf))?;
        for (key, (argc, body)) in ins_defs {
            self.defs.insert(key.into(), InterpValue::Data(argc, body));
        }
        Ok(content)
    }

    pub fn save_to_compfile(&self, compf: &str, content: VAN) -> Result<(), anyhow::Error> {
        let fh = std::fs::File::create(compf)
            .with_context(|| format!("Failed to create compfile '{}'", compf))?;
        let z = flate2::write::DeflateEncoder::new(fh, flate2::Compression::default());
        let codat = CompilateData {
            content,
            defs: self
                .defs
                .iter()
                .filter_map(|i| {
                    if let InterpValue::Data(argc, ref body) = &i.1 {
                        Some((i.0.to_string(), (*argc, body.clone())))
                    } else {
                        None
                    }
                })
                .collect(),
        };
        bincode::serialize_into(z, &codat)
            .with_context(|| format!("Failed to write compfile '{}'", compf))?;
        Ok(())
    }
}

fn conv_to_constant(arg: &ASTNode) -> Option<Atom> {
    Some(match arg {
        ASTNode::Constant(_, x) => x.clone(),
        ASTNode::Grouped(false, x) => {
            let mut impc = x.iter().map(conv_to_constant);
            if x.len() == 1 {
                impc.next().unwrap()?
            } else {
                impc.try_fold(String::new(), |acc, i| i.map(|i| acc + &i))?
                    .into()
            }
        }
        _ => return None,
    })
}

macro_rules! define_blti {
    ($defs:ident, $name_str:tt, $name:ident($args:pat, $ctx:ident) ($ac:expr) $body:tt) => {
        fn $name($args: &mut VAN, mut $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body
        $defs.insert(crulst_atom!($name_str), InterpValue::BuiltIn(Some($ac), BuiltInFn::Manual($name)));
    };
    ($defs:ident, $name_str:tt, $name:ident($args:pat, $ctx:ident) $body:tt) => {
        fn $name($args: &mut VAN, mut $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body
        $defs.insert(crulst_atom!($name_str), InterpValue::BuiltIn(None, BuiltInFn::Manual($name)));
    };
    ($defs:ident, $name_str:tt, $name:ident($args:pat) ($ac:expr) $body:tt) => {
        fn $name($args: VAN) -> Option<ASTNode> $body
        $defs.insert(crulst_atom!($name_str), InterpValue::BuiltIn(Some($ac), BuiltInFn::Automatic($name)));
    };
    ($defs:ident, $name_str:tt, $name:ident($args:pat) $body:tt) => {
        fn $name($args: VAN) -> Option<ASTNode> $body
        $defs.insert(crulst_atom!($name_str), InterpValue::BuiltIn(None, BuiltInFn::Automatic($name)));
    };
}

lazy_static::lazy_static! {
    static ref BUILTINS: DefinesMap = {
        let mut defs = DefinesMap::new();
        define_blti!(defs, "add", add(args) (2) {
            let unpacked = args.into_iter().filter_map(|x| Some(x
                .as_constant()?
                .parse::<i64>()
                .expect("expected number as @param"))).collect::<Vec<_>>();
            if unpacked.len() != 2 {
                // if any argument wasn't evaluated --> dropped --> different len()
                return None;
            }
            Some(ASTNode::Constant(true, (unpacked[0] + unpacked[1]).to_string().into()))
        });
        define_blti!(defs, "def", def(args, ctx) {
            if args.len() < 3 {
                return None;
            }
            let mut unpack = |x: &mut ASTNode| {
                x.eval(&mut ctx);
                x.simplify_inplace();
                conv_to_constant(x)
            };
            let varname = unpack(&mut args[0])?;
            let argc: usize = unpack(&mut args[1])?
                .parse()
                .expect("expected number as argc");
            let value = args[2..].to_vec().lift_ast().simplify();
            ctx.defs
                .insert(varname, InterpValue::Data(argc, value));
            Some(ASTNode::NullNode)
        });
        define_blti!(defs, "include", include(args, ctx) (1) {
            args[0].eval(&mut ctx);
            args[0].simplify_inplace();
            let filename = conv_to_constant(&args[0])?;
            let filename: &str = &filename;
            Some(match ctx.comp_map.get(filename) {
                None => crate::parser::file2ast(filename, ctx.opts),
                Some(compf) => ctx.load_from_compfile(compf),
            }
                .expect("expected valid file")
                .lift_ast())
        });
        define_blti!(defs, "pass", pass(args) {
            Some(args.lift_ast())
        });
        define_blti!(defs, "suppress", suppress(_args) {
            // suppress all direct output from the code section, but evaluate it
            Some(ASTNode::NullNode)
        });
        define_blti!(defs, "une", une(args) {
            // un-escape
            Some(args.into_iter().map(|mut x| {
                if let ASTNode::Grouped(ref mut is_strict, _) = x {
                    *is_strict = false;
                }
                x
            }).collect::<Vec<_>>().lift_ast())
        });
        define_blti!(defs, "unisp", unisp(args) {
            // unify spaces
            Some(args.into_iter().map(|mut x| {
                match &mut x {
                    ASTNode::Constant(false, ref mut dat) => *dat = crulst_atom!(" "),
                    ASTNode::Grouped(ref mut is_strict, _) => *is_strict = false,
                    _ => {},
                }
                x
            }).collect::<Vec<_>>().lift_ast())
        });
        defs
    };
}

fn eval_cmd(cmd: &mut VAN, args: &mut VAN, mut ctx: &mut EvalContext) -> Option<ASTNode> {
    use self::InterpValue::*;
    use crate::mangle_ast::MangleASTExt;

    // evaluate command name
    for i in cmd.iter_mut() {
        i.eval(ctx);
    }
    // allow partial evaluation of command name
    *cmd = cmd.take().simplify().compact_toplevel();
    let cmd = match cmd.clone().lift_ast().simplify() {
        ASTNode::Constant(true, cmd) => cmd,
        _ => return None,
    };

    // evaluate command
    match ctx.defs.get(&cmd)?.clone() {
        BuiltIn(a, x) => match a {
            Some(n) if args.len() != n => None,
            _ => match x {
                BuiltInFn::Manual(y) => y(args, &mut ctx),
                BuiltInFn::Automatic(y) => {
                    for i in args.iter_mut() {
                        i.eval(ctx);
                        i.simplify_inplace();
                    }
                    y(args.clone())
                }
            }
            .map(|i| i.simplify()),
        },
        Data(n, mut x) => {
            for i in args.iter_mut() {
                i.eval(ctx);
            }
            if args.len() != n {
                return None;
            }
            for i in (0..n).rev() {
                x.replace_inplace(&format!("${}", i), &args[i]);
            }
            Some(x.simplify())
        }
    }
}

trait Eval: MangleAST {
    fn eval(&mut self, ctx: &mut EvalContext);
}

impl Eval for ASTNode {
    fn eval(mut self: &mut Self, mut ctx: &mut EvalContext) {
        use ASTNode::*;
        match &mut self {
            CmdEval(cmd, args) => {
                if let Some(x) = eval_cmd(cmd, args, &mut ctx) {
                    *self = x;
                }
            }
            Grouped(_, x) => {
                x.eval(&mut ctx);
            }
            _ => {}
        }
    }
}

impl Eval for VAN {
    fn eval(&mut self, mut ctx: &mut EvalContext) {
        for i in self {
            i.eval(&mut ctx);
        }
    }
}

pub fn eval(
    data: &mut VAN,
    opts: ParserOptions,
    comp_map: &CompilatesMap<'_>,
    comp_out: Option<&str>,
) {
    use crate::mangle_ast::MangleASTExt;
    let mut ctx = EvalContext {
        defs: BUILTINS.clone(),
        opts,
        comp_map,
    };
    let mut cplx = data.get_complexity();
    loop {
        data.eval(&mut ctx);
        *data = data.take().simplify().compact_toplevel();
        let new_cplx = data.get_complexity();
        if new_cplx == cplx {
            break;
        }
        cplx = new_cplx;
    }
    if let Some(comp_out) = comp_out {
        ctx.save_to_compfile(comp_out, data.clone())
            .expect("save failed");
    }
}
