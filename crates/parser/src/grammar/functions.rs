use super::expressions::{parse_call_args, parse_expr};
use super::types::parse_type_desc;

use crate::ast::{
    BinOperator, Expr, FuncStmt, Function, FunctionArg, RegularFunctionArg, VarDeclTarget,
};
use crate::lexer::TokenKind;
use crate::node::{Node, Span};
use crate::{Label, ParseFailed, ParseResult, Parser};

/// Parse a function definition. The optional `pub` qualifier must be parsed by
/// the caller, and passed in. Next token must be `unsafe` or `fn`.
pub fn parse_fn_def(par: &mut Parser, mut pub_qual: Option<Span>) -> ParseResult<Node<Function>> {
    let unsafe_qual = par.optional(TokenKind::Unsafe).map(|tok| tok.span);
    if let Some(pub_) = par.optional(TokenKind::Pub) {
        let unsafe_span =
            unsafe_qual.expect("caller must verify that next token is `unsafe` or `fn`");

        par.fancy_error(
            "`pub` visibility modifier must come before `unsafe`",
            vec![Label::primary(
                unsafe_span + pub_.span,
                "use `pub unsafe` here",
            )],
            vec![],
        );
        pub_qual = pub_qual.or(Some(pub_.span));
    }
    let fn_tok = par.expect(TokenKind::Fn, "failed to parse function definition")?;
    let name = par.expect(TokenKind::Name, "failed to parse function definition")?;
    let mut span = fn_tok.span + unsafe_qual + pub_qual + name.span;

    let args = match par.peek_or_err()? {
        TokenKind::ParenOpen => {
            let node = parse_fn_param_list(par)?;
            span += node.span;
            node.kind
        }
        TokenKind::Colon | TokenKind::Arrow => {
            par.fancy_error(
                "function definition requires a list of parameters",
                vec![Label::primary(
                    name.span,
                    "function name must be followed by `(`",
                )],
                vec![
                    format!(
                        "Note: if the function `{}` takes no parameters, use an empty set of parentheses.",
                        name.text
                    ),
                    format!("Example: fn {}()", name.text),
                    "Note: each parameter must have a name and a type.".into(),
                    format!("Example: fn {}(my_value: u256, x: bool)", name.text),
                ],
            );
            vec![]
        }
        _ => {
            let tok = par.next()?;
            par.unexpected_token_error(
                tok.span,
                "failed to parse function definition",
                vec![
                    "function name must be followed by a list of parameters".into(),
                    "Example: `fn foo(x: address, y: u256)` or `fn f()`".into(),
                ],
            );
            return Err(ParseFailed);
        }
    };
    let return_type = if par.peek() == Some(TokenKind::Arrow) {
        par.next()?;
        Some(parse_type_desc(par)?)
    } else {
        None
    };
    span += return_type.as_ref();

    // TODO: allow multi-line return type? `fn f()\n ->\n u8`
    // TODO: allow single-line fn defs?
    par.enter_block(span, "function definition")?;
    let body = parse_block_stmts(par)?;
    span += body.last();
    Ok(Node::new(
        Function {
            pub_: pub_qual,
            unsafe_: unsafe_qual,
            name: name.into(),
            args,
            return_type,
            body,
        },
        span,
    ))
}

fn parse_fn_param_list(par: &mut Parser) -> ParseResult<Node<Vec<Node<FunctionArg>>>> {
    let mut span = par.assert(TokenKind::ParenOpen).span;
    let mut params = vec![];
    loop {
        match par.peek_or_err()? {
            TokenKind::ParenClose => {
                span += par.next()?.span;
                break;
            }
            TokenKind::Name | TokenKind::SelfValue => {
                let name = par.next()?;

                if name.kind == TokenKind::SelfValue {
                    params.push(Node::new(FunctionArg::Zelf, name.span));
                } else {
                    par.expect_with_notes(
                        TokenKind::Colon,
                        "failed to parse function parameter",
                        |_| {
                            vec![
                                "Note: parameter name must be followed by a colon and a type description"
                                    .into(),
                                format!("Example: `{}: u256`", name.text),
                            ]
                        },
                    )?;
                    let typ = parse_type_desc(par)?;
                    let param_span = name.span + typ.span;
                    params.push(Node::new(
                        FunctionArg::Regular(RegularFunctionArg {
                            name: Node::new(name.text.into(), name.span),
                            typ,
                        }),
                        param_span,
                    ));
                }

                if par.peek() == Some(TokenKind::Comma) {
                    par.next()?;
                } else {
                    span += par
                        .expect(
                            TokenKind::ParenClose,
                            "unexpected token while parsing function parameter list",
                        )?
                        .span;
                    break;
                }
            }
            _ => {
                let tok = par.next()?;
                par.unexpected_token_error(
                    tok.span,
                    "failed to parse function parameter list",
                    vec![],
                );
                return Err(ParseFailed);
            }
        }
    }
    Ok(Node::new(params, span))
}

/// Parse (function) statements until a block dedent or end-of-file is reached.
fn parse_block_stmts(par: &mut Parser) -> ParseResult<Vec<Node<FuncStmt>>> {
    let mut body = vec![];
    loop {
        match par.peek() {
            None => break,
            Some(TokenKind::Dedent) => {
                par.next()?;
                break;
            }
            Some(_) => body.push(parse_stmt(par)?),
        }
    }
    Ok(body)
}

fn aug_assign_op(tk: TokenKind) -> Option<BinOperator> {
    use BinOperator::*;
    use TokenKind::*;

    let op = match tk {
        PlusEq => Add,
        MinusEq => Sub,
        StarEq => Mult,
        SlashEq => Div,
        PercentEq => Mod,
        StarStarEq => Pow,
        LtLtEq => LShift,
        GtGtEq => RShift,
        PipeEq => BitOr,
        HatEq => BitXor,
        AmperEq => BitAnd,
        _ => return None,
    };
    Some(op)
}

/// Parse a `continue`, `break`, `pass`, or `revert` statement.
///
/// # Panics
/// Panics if the next token isn't one of the above.
pub fn parse_single_word_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let tok = par.next()?;
    par.expect_newline(tok.kind.describe())?;
    let stmt = match tok.kind {
        TokenKind::Continue => FuncStmt::Continue,
        TokenKind::Break => FuncStmt::Break,
        TokenKind::Pass => FuncStmt::Pass,
        _ => panic!(),
    };
    Ok(Node::new(stmt, tok.span))
}

/// Parse a function-level statement.
pub fn parse_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    use TokenKind::*;

    // rule: stmt parsing fns eat the trailing separator (newline, semi, eof)
    match par.peek_or_err()? {
        For => parse_for_stmt(par),
        If => parse_if_stmt(par),
        While => parse_while_stmt(par),
        Return => parse_return_stmt(par),
        Assert => parse_assert_stmt(par),
        Revert => parse_revert_stmt(par),
        Continue | Break | Pass => parse_single_word_stmt(par),
        Emit => parse_emit_statement(par),
        Let => parse_var_decl(par),
        Unsafe => parse_unsafe_block(par),
        _ => parse_expr_stmt(par),
    }
}

fn parse_var_decl(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let let_tkn = par.assert(TokenKind::Let);
    let expr = parse_expr(par)?;
    let target = expr_to_vardecl_target(par, expr.clone())?;
    let node = match par.peek() {
        Some(TokenKind::Colon) => {
            par.next()?;
            let typ = parse_type_desc(par)?;
            let value = if par.peek() == Some(TokenKind::Eq) {
                par.next()?;
                Some(parse_expr(par)?)
            } else {
                None
            };
            let span = let_tkn.span + target.span + typ.span + value.as_ref();
            par.expect_newline("variable declaration")?;
            Node::new(FuncStmt::VarDecl { target, typ, value }, span)
        }
        _ => {
            par.fancy_error(
                "failed to parse variable declaration",
                vec![Label::primary(
                    expr.span,
                    "Must be followed by type annotation",
                )],
                vec!["Example: `let x: u8 = 1`".into()],
            );
            return Err(ParseFailed);
        }
    };
    Ok(node)
}

/// Parse a (function) statement that begins with an expression. This might be
/// a `VarDecl`, `Assign`, or an expression.
fn parse_expr_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    use TokenKind::*;
    let expr = parse_expr(par)?;
    let node = match par.peek() {
        None | Some(Newline) => {
            let span = expr.span;
            Node::new(FuncStmt::Expr { value: expr }, span)
        }
        Some(Colon) => {
            par.fancy_error(
                "Variable declaration must begin with `let`",
                vec![Label::primary(expr.span, "invalid variable declaration")],
                vec!["Example: `let x: u8 = 1`".into()],
            );
            return Err(ParseFailed);
        }
        Some(Eq) => {
            par.next()?;
            let value = parse_expr(par)?;
            let span = expr.span + value.span;
            // TODO: should `x = y = z` be allowed?
            Node::new(
                FuncStmt::Assign {
                    target: expr,
                    value,
                },
                span,
            )
        }
        Some(tk) => {
            let tok = par.next()?;
            if let Some(op) = aug_assign_op(tk) {
                let value = parse_expr(par)?;
                let span = expr.span + value.span;
                Node::new(
                    FuncStmt::AugAssign {
                        target: expr,
                        op: Node::new(op, tok.span),
                        value,
                    },
                    span,
                )
            } else {
                par.unexpected_token_error(tok.span, "invalid syntax in function body", vec![]);
                return Err(ParseFailed);
            }
        }
    };
    par.expect_newline("statement")?;
    Ok(node)
}

fn expr_to_vardecl_target(par: &mut Parser, expr: Node<Expr>) -> ParseResult<Node<VarDeclTarget>> {
    match expr.kind {
        Expr::Name(name) => Ok(Node::new(VarDeclTarget::Name(name), expr.span)),
        Expr::Tuple { elts } if !elts.is_empty() => Ok(Node::new(
            VarDeclTarget::Tuple(
                elts.into_iter()
                    .map(|elt| expr_to_vardecl_target(par, elt))
                    .collect::<ParseResult<Vec<_>>>()?,
            ),
            expr.span,
        )),
        _ => {
            par.fancy_error(
                "failed to parse variable declaration",
                vec![Label::primary(
                    expr.span,
                    "invalid variable declaration target",
                )],
                vec![
                    "The left side of a variable declaration can be either a name\nor a non-empty tuple."
                        .into(),
                ],
            );
            Err(ParseFailed)
        }
    }
}

/// Parse an `if` statement, or an `elif` block.
///
/// # Panics
/// Panics if the next token isn't `if` or `elif`.
pub fn parse_if_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let if_tok = par.next()?;
    assert!(matches!(if_tok.kind, TokenKind::If | TokenKind::Elif));

    let test = parse_expr(par)?;
    par.enter_block(if_tok.span + test.span, "`if` statement")?;
    let body = parse_block_stmts(par)?;

    let else_block = match par.peek() {
        Some(TokenKind::Else) => {
            let else_tok = par.next()?;
            par.enter_block(else_tok.span, "`if` statement `else` branch")?;
            parse_block_stmts(par)?
        }
        Some(TokenKind::Elif) => {
            vec![parse_if_stmt(par)?]
        }
        _ => vec![],
    };

    let span = if_tok.span + test.span + body.last() + else_block.last();
    Ok(Node::new(
        FuncStmt::If {
            test,
            body,
            or_else: else_block,
        },
        span,
    ))
}

/// Parse a `while` statement.
///
/// # Panics
/// Panics if the next token isn't `while`.
pub fn parse_while_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let while_tok = par.assert(TokenKind::While);

    let test = parse_expr(par)?;
    par.enter_block(while_tok.span + test.span, "`while` statement")?;
    let body = parse_block_stmts(par)?;
    let span = while_tok.span + test.span + body.last();

    Ok(Node::new(FuncStmt::While { test, body }, span))
}

/// Parse a `for` statement.
///
/// # Panics
/// Panics if the next token isn't `for`.
pub fn parse_for_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let for_tok = par.assert(TokenKind::For);

    let target = par
        .expect(TokenKind::Name, "failed to parse `for` statement")?
        .into();
    par.expect(TokenKind::In, "failed to parse `for` statement")?;
    let iter = parse_expr(par)?;
    par.enter_block(for_tok.span + iter.span, "`for` statement")?;
    let body = parse_block_stmts(par)?;
    let span = for_tok.span + iter.span + body.last();

    Ok(Node::new(FuncStmt::For { target, iter, body }, span))
}

/// Parse a `return` statement.
///
/// # Panics
/// Panics if the next token isn't `return`.
pub fn parse_return_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let ret = par.assert(TokenKind::Return);
    let value = match par.peek() {
        None | Some(TokenKind::Newline) => None,
        Some(_) => Some(parse_expr(par)?),
    };
    par.expect_newline("return statement")?;
    let span = ret.span + value.as_ref();
    Ok(Node::new(FuncStmt::Return { value }, span))
}

/// Parse an `assert` statement.
///
/// # Panics
/// Panics if the next token isn't `assert`.
pub fn parse_assert_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let assert_tok = par.assert(TokenKind::Assert);
    let test = parse_expr(par)?;
    let msg = match par.peek() {
        None | Some(TokenKind::Newline) => None,
        Some(TokenKind::Comma) => {
            par.next()?;
            Some(parse_expr(par)?)
        }
        Some(_) => {
            let tok = par.next()?;
            par.unexpected_token_error(tok.span, "failed to parse `assert` statement", vec![]);
            return Err(ParseFailed);
        }
    };
    par.expect_newline("assert statement")?;
    let span = assert_tok.span + test.span + msg.as_ref();
    Ok(Node::new(FuncStmt::Assert { test, msg }, span))
}

/// Parse a `revert` statement.
///
/// # Panics
/// Panics if the next token isn't `revert`.
pub fn parse_revert_stmt(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let revert_tok = par.assert(TokenKind::Revert);
    let error = match par.peek() {
        None | Some(TokenKind::Newline) => None,
        Some(_) => Some(parse_expr(par)?),
    };
    par.expect_newline("revert statement")?;
    let span = revert_tok.span + error.as_ref();
    Ok(Node::new(FuncStmt::Revert { error }, span))
}

/// Parse an `emit` statement
///
/// # Panics
/// Panics if the next token isn't `emit`
pub fn parse_emit_statement(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let emit_tok = par.assert(TokenKind::Emit);
    let event_name = par.expect(TokenKind::Name, "failed to parse emit statement")?;

    if let Some(TokenKind::ParenOpen) = par.peek() {
        let args = parse_call_args(par)?;
        par.expect_newline("emit statement")?;
        let span = emit_tok.span + args.span;
        return Ok(Node::new(
            FuncStmt::Emit {
                name: event_name.into(),
                args,
            },
            span,
        ));
    } else {
        par.expect(
            TokenKind::ParenOpen,
            "failed to parse event invocation parameter list",
        )?;
    }

    Err(ParseFailed)
}

/// Parse an `unsafe` block.
///
/// # Panics
/// Panics if the next token isn't `unsafe`.
pub fn parse_unsafe_block(par: &mut Parser) -> ParseResult<Node<FuncStmt>> {
    let kw_tok = par.assert(TokenKind::Unsafe);
    par.enter_block(kw_tok.span, "`unsafe` block")?;
    let body = parse_block_stmts(par)?;
    let span = kw_tok.span + body.last();

    Ok(Node::new(FuncStmt::Unsafe(body), span))
}
