use crate::ast::{ASTNode, CmdEvalArgs, GroupType, VAN};
use bstr::ByteSlice;

// === parser options

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParserOptions {
    escc: u8,
    pass_escc: bool,
}

impl ParserOptions {
    #[inline]
    pub const fn new(escc: u8, pass_escc: bool) -> Self {
        Self { escc, pass_escc }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum ParserErrorDetail {
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("got empty/invalid eval statement")]
    InvalidEval,
    #[error("expected '{}' instead", char::from(*.0))]
    ExpectedInstead(u8),

    /// Escaped end-of-scope markers are dangerous, because they probably don't
    /// do, what you would naively expect. The correct way to escape them is
    /// to *not* escape the corresponding begin-of-scope marker, which thus
    /// constitutes a Group.
    #[error("dangerous escaped end-of-scope marker ('{}')", char::from(*.0))]
    DangerousEos(u8),
    #[error("unexpected unbalanced end-of-scope marker ('{}')", char::from(*.0))]
    UnbalancedEos(u8),
}

type PED = ParserErrorDetail;

pub struct ParserError<'a> {
    origin: &'a [u8],
    offending: &'a [u8],
    detail: PED,
}

// === parse trait

trait Parse: Sized {
    /// # Return value
    /// * `Ok(rest, parsed_obj)`
    /// * `Err(offending_code, description)`
    fn parse(data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>>;
}

// === parser utils

fn get_offset_of<T>(whole_buffer: &T, part: &T) -> usize
where
    T: AsRef<[u8]> + ?Sized,
{
    // NOTE: use offset_from() once it's stable
    part.as_ref().as_ptr() as usize - whole_buffer.as_ref().as_ptr() as usize
}

fn str_slice_between<'a>(whole_buffer_start: &'a [u8], post_part: &'a [u8]) -> &'a [u8] {
    &whole_buffer_start[..get_offset_of(whole_buffer_start, post_part)]
}

lazy_static::lazy_static! {
    static ref SCOPE_MARKERS: std::collections::HashMap<u8, (u8, GroupType)> = maplit::hashmap! {
        b'(' => (b')', GroupType::Strict),
        b'{' => (b'}', GroupType::Loose),
    };
}

fn is_scope_end(x: &u8) -> bool {
    SCOPE_MARKERS.values().any(|(eogm, _)| eogm == x)
}

/// 1. part while f(x) == true, then 2. part
fn str_split_at_while(x: &[u8], f: impl FnMut(&u8) -> bool) -> (&[u8], &[u8]) {
    x.split_at(x.bytes().take_while(f).count())
}

/// escaped escape symbol or other escaped code: optional passthrough
fn parse_escaped_const(i: u8, opts: ParserOptions) -> Option<ASTNode> {
    match i {
        b'{' | b'}' | b'$' => {}
        b'\n' => return Some(ASTNode::NullNode),
        _ => {
            if i != opts.escc {
                return None;
            }
        }
    }
    let mut ret = Vec::with_capacity(2);
    if opts.pass_escc {
        ret.push(opts.escc);
    }
    ret.push(i);
    Some(ASTNode::Constant(ret.into()))
}

fn str_split_at_ctrl(
    data: &[u8],
    opts: ParserOptions,
    f_do_cont_at: impl Fn(&u8) -> bool,
) -> (&[u8], &[u8]) {
    str_split_at_while(data, |i| match i {
        b'$' | b'(' | b')' | b'{' | b'}' => false,
        _ => *i != opts.escc && f_do_cont_at(i),
    })
}

fn do_expect<'a>(origin: &'a [u8], rest: &'a [u8], c: u8) -> Result<&'a [u8], ParserError<'a>> {
    if rest.get(0) == Some(&c) {
        Ok(&rest[1..])
    } else {
        Err(ParserError {
            origin,
            offending: rest,
            detail: PED::ExpectedInstead(c),
        })
    }
}

impl Parse for ASTNode {
    fn parse(data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>> {
        let escc = opts.escc;
        let mut iter = data.iter();

        let i = *iter.next().ok_or_else(|| ParserError {
            origin: data,
            offending: data,
            detail: PED::UnexpectedEof,
        })?;
        match i {
            _ if i == escc => {
                let i = *iter.next().ok_or_else(|| ParserError {
                    origin: data,
                    offending: data,
                    detail: PED::UnexpectedEof,
                })?;
                if i == b'(' {
                    // got begin of cmdeval block
                    let (rest, mut vanx) = VAN::parse(iter.as_slice(), opts)?;
                    if vanx.is_empty() {
                        return Err(ParserError {
                            origin: data,
                            offending: &data[..std::cmp::min(data.len(), 3)],
                            detail: PED::InvalidEval,
                        });
                    }
                    let rest = do_expect(data, rest, /*(*/ b')')?;

                    // extract command
                    assert!(!vanx.is_empty());
                    let split_point = vanx
                        .iter()
                        .enumerate()
                        .filter_map(|y| if y.1.is_space() { Some(y.0 + 1) } else { None })
                        .next()
                        .unwrap_or(1);
                    let van = vanx.split_off(split_point);
                    let mut cmd = vanx;
                    if cmd.last().unwrap().is_space() {
                        cmd.pop();
                    }
                    Ok((
                        rest,
                        ASTNode::CmdEval {
                            cmd,
                            args: CmdEvalArgs::from_wsdelim(van),
                        },
                    ))
                } else if let Some(c) = parse_escaped_const(i, opts) {
                    Ok((iter.as_slice(), c))
                } else if is_scope_end(&i) {
                    Err(ParserError {
                        origin: data,
                        offending: str_slice_between(data, iter.as_slice()),
                        detail: PED::DangerousEos(i),
                    })
                } else {
                    // interpret it as a command (LaTeX-alike)
                    let (cmd, mut rest) =
                        str_split_at_ctrl(&data[1..], opts, |x| !x.is_ascii_whitespace());
                    if cmd.is_empty() {
                        return Err(ParserError {
                            origin: data,
                            offending: str_slice_between(data, iter.as_slice()),
                            detail: PED::InvalidEval,
                        });
                    }
                    let args = if rest.get(0) == Some(&b'(') {
                        let (tmp_rest, van) = VAN::parse(&rest[1..], opts)?;
                        rest = do_expect(data, tmp_rest, b')')?;
                        CmdEvalArgs::from_wsdelim(van)
                    } else {
                        Default::default()
                    };
                    Ok((
                        rest,
                        ASTNode::CmdEval {
                            cmd: vec![ASTNode::Constant(cmd.into())],
                            args,
                        },
                    ))
                }
            }
            b'$' => {
                let (cdat, rest) = str_split_at_while(iter.as_slice(), |&i| i == b'$');
                let (idxs, rest) = str_split_at_while(rest, u8::is_ascii_digit);
                Ok((
                    rest,
                    ASTNode::Argument {
                        indirection: cdat.len(),
                        index: atoi::atoi(idxs),
                    },
                ))
            }
            _ if is_scope_end(&i) => Err(ParserError {
                origin: data,
                offending: str_slice_between(data, iter.as_slice()),
                detail: PED::UnbalancedEos(i),
            }),
            _ => Ok(if let Some(&(eogm, typ)) = SCOPE_MARKERS.get(&i) {
                let (rest, elems) = VAN::parse(iter.as_slice(), opts)?;
                (
                    do_expect(data, rest, eogm)?,
                    ASTNode::Grouped { typ, elems },
                )
            } else {
                let is_whitespace = i.is_ascii_whitespace();
                let (cdat, rest) =
                    str_split_at_ctrl(data, opts, |x| x.is_ascii_whitespace() == is_whitespace);
                (rest, ASTNode::Constant(cdat.into()))
            }),
        }
    }
}

impl Parse for VAN {
    fn parse(mut data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>> {
        let mut ret = VAN::new();
        while data.get(0).map(is_scope_end) == Some(false) {
            let (rest, node) = ASTNode::parse(data, opts)?;
            ret.push(node);
            data = rest;
        }
        Ok((data, ret))
    }
}

// === main parser

/// At top level, only parse things inside CmdEval's
pub fn parse_toplevel(mut data: &[u8], opts: ParserOptions) -> Result<VAN, ParserError<'_>> {
    let mut ret = VAN::new();
    while !data.is_empty() {
        let (cstp, rest) = str_split_at_while(data, |&i| i != opts.escc);
        if !cstp.is_empty() {
            ret.push(ASTNode::Constant(cstp.into()));
        }
        if rest.is_empty() {
            break;
        }
        let (rest, node) = ASTNode::parse(rest, opts)?;
        ret.push(node);
        data = rest;
    }
    Ok(ret)
}

pub fn file2ast(filename: &std::path::Path, opts: ParserOptions) -> Result<VAN, anyhow::Error> {
    use anyhow::Context;

    let fh = readfilez::read_from_file(std::fs::File::open(filename))
        .with_context(|| format!("unable to read file '{}'", filename.display()))?;
    let input = fh.as_slice();

    parse_toplevel(input, opts).map_err(|e| {
        use std::str::FromStr;

        let start_pos = get_offset_of(input, e.offending);
        let start_pos_origin = get_offset_of(input, e.origin);
        let start_pos_origin = if start_pos == start_pos_origin {
            None
        } else {
            Some(start_pos_origin)
        };

        if let Ok(input) = std::str::from_utf8(input) {
            use codespan_reporting::{
                diagnostic::{Diagnostic, Label},
                term,
            };

            let mut files = codespan::Files::new();
            let fileid = files.add(filename, input);

            let mut labels = vec![Label::primary(
                fileid,
                start_pos..(start_pos + e.offending.len()),
            )];
            if let Some(spo) = start_pos_origin {
                labels.push(
                    Label::secondary(fileid, spo..start_pos)
                        .with_message("error origin / parsed prefix"),
                );
            }

            term::emit(
                &mut term::termcolor::StandardStream::stderr(
                    term::ColorArg::from_str("auto").unwrap().into(),
                )
                .lock(),
                &term::Config::default(),
                &files,
                &Diagnostic::error()
                    .with_message(e.detail.to_string())
                    .with_labels(labels),
            )
            .unwrap();
        } else {
            use ansi_term::{Colour, Style};
            let x_bold = Style::new().bold();
            let x_red = x_bold.clone().fg(Colour::Red);
            let x_warn = x_bold.clone().fg(Colour::Yellow);
            let x_note = x_bold.clone().fg(Colour::Blue);

            println!(
                "{}crulz: {}warning{}: {}: file contains non-UTF-8 data",
                x_bold.prefix(),
                x_bold.infix(x_warn),
                x_warn.infix(x_bold),
                filename.display()
            );

            eprintln!(
                "crulz: {}error{}: {}: {}..{}: {}{}",
                x_bold.infix(x_red),
                x_red.infix(x_bold),
                filename.display(),
                start_pos,
                start_pos + e.offending.len(),
                e.detail,
                x_bold.suffix(),
            );

            eprintln!(
                "\t{}: {}..{}: {:?}",
                filename.display(),
                start_pos,
                start_pos + e.offending.len(),
                <&bstr::BStr>::from(e.offending),
            );

            if let Some(spo) = start_pos_origin {
                eprintln!(
                    "{}crulz: {}note{}: {}: error origin is at offset {}{}",
                    x_bold.prefix(),
                    x_bold.infix(x_note),
                    x_note.infix(x_bold),
                    filename.display(),
                    spo,
                    x_bold.suffix()
                );
            }
        }
        anyhow::anyhow!("{}", e.detail)
    })
}
