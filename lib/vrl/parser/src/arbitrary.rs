//! Arbitrary instances for the AST nodes.
//! This allows us to generate random VRL nodes for the purposes of fuzz testing.
//! We put as little effort as is reasonable to generate valid programs. The nodes are passed to the
//! compiler which rejects invalid scripts. This prevents us from imposing too much structure on the
//! code that builds in assumptions we have made about the code which gives the fuzzer more freedom
//! to find areas of nodes that we haven't thought of.
use crate::{
    arbitrary_depth::ArbitraryDepth,
    ast::{
        Assignment, AssignmentOp, AssignmentTarget, Block, Expr, Ident, IfStatement, Node, Op,
        Opcode, Predicate, RootExpr,
    },
    Literal, Program,
};
use arbitrary::{Arbitrary, Unstructured};
use diagnostic::Span;
use lookup::LookupBuf;

const DEPTH: isize = 4;

impl<'a> Arbitrary<'a> for RootExpr {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(RootExpr::Expr(Node::new(
            Span::default(),
            Expr::arbitrary_depth(u, DEPTH)?,
        )))
    }
}

impl<'a> Arbitrary<'a> for Program {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // limit to 15 statements to avoid overwhelming the compiler.
        let len = usize::arbitrary(u)? % 15;
        let statements = (0..len)
            .map(|_| Ok(Node::new(Span::default(), RootExpr::arbitrary(u)?)))
            .collect::<arbitrary::Result<Vec<_>>>()?;

        Ok(Program(statements))
    }
}

impl<'a> ArbitraryDepth<'a> for Expr {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        match u8::arbitrary(u)? % 5 {
            0 if depth > 0 => Ok(Expr::Op(Node::new(
                Span::default(),
                Op::arbitrary_depth(u, depth - 1)?,
            ))),
            1 if depth == DEPTH => Ok(Expr::Assignment(Node::new(
                Span::default(),
                Assignment::arbitrary_depth(u, depth - 1)?,
            ))),
            2 => Ok(Expr::Variable(Node::new(
                Span::default(),
                Ident::arbitrary(u)?,
            ))),
            3 if depth > 0 => Ok(Expr::IfStatement(Node::new(
                Span::default(),
                IfStatement::arbitrary_depth(u, depth - 1)?,
            ))),
            _ => Ok(Expr::Literal(Node::new(
                Span::default(),
                Literal::arbitrary(u)?,
            ))),
        }
    }
}

impl<'a> Arbitrary<'a> for Ident {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Limit the number of identifiers to increase the chances of reusing the same one in the generated program.
        let path = match u8::arbitrary(u)? % 3 {
            0 => "noog",
            1 => "flork",
            _ => "shning",
        };

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
            .map(|_| {
                Ok(Node::new(
                    Span::default(),
                    Expr::arbitrary_depth(u, depth - 1)?,
                ))
            })
            .collect::<arbitrary::Result<Vec<_>>>()?;

        Ok(Block(statements))
    }
}

impl<'a> ArbitraryDepth<'a> for IfStatement {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let predicate = Predicate::arbitrary_depth(u, depth - 1)?;
        let consequent = Block::arbitrary_depth(u, depth - 1)?;
        let alternative = Block::arbitrary_depth(u, depth - 1)?;

        Ok(IfStatement {
            predicate: Node::new(Span::default(), predicate),
            consequent: Node::new(Span::default(), consequent),
            alternative: Some(Node::new(Span::default(), alternative)),
        })
    }
}

impl<'a> ArbitraryDepth<'a> for Predicate {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        Ok(Predicate::One(Box::new(Node::new(
            Span::default(),
            Expr::arbitrary_depth(u, depth - 1)?,
        ))))
    }
}

impl<'a> ArbitraryDepth<'a> for Op {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let left = Expr::arbitrary_depth(u, depth - 1)?;
        let right = Expr::arbitrary_depth(u, depth - 1)?;
        let opcode = Opcode::arbitrary(u)?;

        Ok(Op(
            Box::new(Node::new(Span::default(), left)),
            Node::new(Span::default(), opcode),
            Box::new(Node::new(Span::default(), right)),
        ))
    }
}

impl<'a> ArbitraryDepth<'a> for Assignment {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self> {
        let target = Node::new(Span::new(0, 5), AssignmentTarget::arbitrary(u)?);
        let op = AssignmentOp::arbitrary(u)?;
        let expr = Box::new(Node::new(
            Span::new(7, 10),
            Expr::arbitrary_depth(u, depth - 1)?,
        ));
        Ok(Assignment::Single { target, op, expr })
    }
}

impl<'a> Arbitrary<'a> for AssignmentTarget {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let path = match u8::arbitrary(u)? % 3 {
            0 => "noog",
            1 => "flork",
            _ => "shning",
        };

        Ok(AssignmentTarget::External(Some(
            LookupBuf::from_str(path).unwrap(),
        )))
    }
}
