//! Arbitrary instances for the AST nodes.
//! This allows us to generate random VRL nodes for the purposes of fuzz testing.
//! We put as little effort as is reasonable to generate valid programs. The nodes are passed to the
//! compiler which rejects invalid scripts. This prevents us from imposing too much structure on the
//! code that builds in assumptions we have made about the code which gives the fuzzer more freedom
//! to find areas of nodes that we haven't thought of.
use crate::{
    arbitrary_depth::ArbitraryDepth,
    ast::{
        Assignment, AssignmentOp, AssignmentTarget, Block, Container, Expr, FunctionArgument,
        FunctionCall, Group, Ident, IfStatement, Node, Op, Opcode, Predicate, Query, QueryTarget,
        RootExpr,
    },
    Literal, Program,
};
use arbitrary::{Arbitrary, Unstructured};
use diagnostic::Span;
use lookup::LookupBuf;

const DEPTH: isize = 4;

/// Choose a random item from the given array.
/// (Make sure the array doesn't exceed 255 items.)
fn choose<'a, 'b, T>(u: &mut Unstructured<'a>, choices: &'b [T]) -> arbitrary::Result<&'b T> {
    let idx = u8::arbitrary(u)? % choices.len() as u8;
    Ok(&choices[idx as usize])
}

fn node<T>(expr: T) -> Node<T> {
    Node::new(Span::default(), expr)
}

impl<'a> Arbitrary<'a> for RootExpr {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(RootExpr::Expr(node(Expr::arbitrary_depth(u, DEPTH)?)))
    }
}

impl<'a> Arbitrary<'a> for Program {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // limit to 15 statements to avoid overwhelming the compiler.
        let len = usize::arbitrary(u)? % 15;
        let statements = (0..len)
            .map(|_| Ok(node(RootExpr::arbitrary(u)?)))
            .collect::<arbitrary::Result<Vec<_>>>()?;

        Ok(Program(statements))
    }
}

impl<'a> ArbitraryDepth<'a> for Expr {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        match u8::arbitrary(u)? % 7 {
            0 if depth > 0 => Ok(Expr::Op(node(Op::arbitrary_depth(u, depth - 1)?))),
            1 if depth == DEPTH => Ok(Expr::Assignment(node(Assignment::arbitrary_depth(
                u,
                depth - 1,
            )?))),
            2 => Ok(Expr::Variable(node(Ident::arbitrary(u)?))),
            3 if depth > 0 => Ok(Expr::IfStatement(node(IfStatement::arbitrary_depth(
                u,
                depth - 1,
            )?))),
            4 => Ok(Expr::Container(node(Container::arbitrary_depth(
                u,
                depth - 1,
            )?))),
            5 => Ok(Expr::Query(node(Query::arbitrary_depth(u, depth - 1)?))),
            _ => Ok(Expr::Literal(node(Literal::arbitrary(u)?))),
        }
    }
}

impl<'a> Arbitrary<'a> for Ident {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Limit the number of identifiers to increase the chances of reusing the same one in the generated program.
        let path = choose(u, &["noog", "flork", "shning"])?;
        Ok(Ident(path.to_string()))
    }
}

impl<'a> Arbitrary<'a> for Literal {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let choice = usize::arbitrary(u)? % 100;
        Ok(match choice {
            0..=5 => Literal::String(String::arbitrary(u)?),
            6..=10 => Literal::Boolean(bool::arbitrary(u)?),
            // TODO Limit the size of integers and keep them positive so both VRLs can handle it.
            _ => Literal::Integer((i64::arbitrary(u)? % 100).abs()),
        })
    }
}

impl<'a> ArbitraryDepth<'a> for Block {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let len = usize::arbitrary(u)? % 5;
        let statements = (0..len)
            .map(|_| Ok(node(Expr::arbitrary_depth(u, depth - 1)?)))
            .collect::<arbitrary::Result<Vec<_>>>()?;

        Ok(Block(statements))
    }
}

impl<'a> ArbitraryDepth<'a> for IfStatement {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let predicate = Predicate::arbitrary_depth(u, depth - 1)?;
        let consequent = Block::arbitrary_depth(u, depth - 1)?;
        let alternative = if bool::arbitrary(u)? {
            Some(Block::arbitrary_depth(u, depth - 1)?)
        } else {
            None
        };

        Ok(IfStatement {
            predicate: node(predicate),
            consequent: node(consequent),
            alternative: alternative.map(node),
        })
    }
}

impl<'a> ArbitraryDepth<'a> for Predicate {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        Ok(Predicate::One(Box::new(node(Expr::arbitrary_depth(
            u,
            depth - 1,
        )?))))
    }
}

impl<'a> ArbitraryDepth<'a> for Op {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let left = Expr::arbitrary_depth(u, depth - 1)?;
        let right = Expr::arbitrary_depth(u, depth - 1)?;
        let opcode = Opcode::arbitrary(u)?;

        Ok(Op(
            Box::new(node(left)),
            node(opcode),
            Box::new(node(right)),
        ))
    }
}

impl<'a> ArbitraryDepth<'a> for Assignment {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        // Assignment panics if the spans overlap.
        let target = Node::new(Span::new(0, 5), AssignmentTarget::arbitrary(u)?);
        let op = AssignmentOp::arbitrary(u)?;
        let expr = Box::new(Node::new(
            Span::new(7, 10),
            Expr::arbitrary_depth(u, depth - 1)?,
        ));

        if u8::arbitrary(u)? % 2 == 0 {
            Ok(Assignment::Single { target, op, expr })
        } else {
            let err = Node::new(Span::new(0, 5), AssignmentTarget::arbitrary(u)?);
            Ok(Assignment::Infallible {
                ok: target,
                err,
                op,
                expr,
            })
        }
    }
}

impl<'a> Arbitrary<'a> for AssignmentTarget {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let path = choose(u, &["noog", "flork", "shning"])?;

        Ok(AssignmentTarget::External(Some(
            LookupBuf::from_str(path).unwrap(),
        )))
    }
}

impl<'a> ArbitraryDepth<'a> for Container {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        match u8::arbitrary(u)? % 2 {
            0 => Ok(Container::Group(Box::new(node(Group::arbitrary_depth(
                u,
                depth - 1,
            )?)))),
            _ => Ok(Container::Block(node(Block::arbitrary_depth(
                u,
                depth - 1,
            )?))),
        }
    }
}

impl<'a> ArbitraryDepth<'a> for Group {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        Ok(Group(node(Expr::arbitrary_depth(u, depth - 1)?)))
    }
}

impl<'a> ArbitraryDepth<'a> for Query {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let target = node(QueryTarget::arbitrary_depth(u, depth - 1)?);
        let path = node(LookupBuf::from_str("thing").unwrap());

        Ok(Self { target, path })
    }
}

impl<'a> ArbitraryDepth<'a> for QueryTarget {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        match u8::arbitrary(u)? % 4 {
            0 => Ok(QueryTarget::Internal(Ident::arbitrary(u)?)),
            1 => Ok(QueryTarget::External),
            2 => Ok(QueryTarget::FunctionCall(FunctionCall::arbitrary_depth(
                u,
                depth - 1,
            )?)),
            _ => Ok(QueryTarget::Container(Container::arbitrary_depth(
                u,
                depth - 1,
            )?)),
        }
    }
}

impl<'a> ArbitraryDepth<'a> for FunctionCall {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let function = choose(u, &["encode_json", "upcase", "downcase"])?;
        let ident = node(Ident(function.to_string()));
        let abort_on_error = false;
        let arguments = vec![node(FunctionArgument {
            ident: None,
            expr: node(Expr::arbitrary_depth(u, depth - 1)?),
        })];

        Ok(Self {
            ident,
            abort_on_error,
            arguments,
        })
    }
}
