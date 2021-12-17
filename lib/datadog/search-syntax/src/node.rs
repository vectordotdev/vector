use regex::Regex;

use crate::grammar::{unescape, DEFAULT_FIELD};

/// This enum represents value comparisons that Queries might perform
#[derive(Debug, Copy, Clone)]
pub enum Comparison {
    /// Greater than.
    Gt,
    /// Less than.
    Lt,
    /// Greater-or-equal-to.
    Gte,
    /// Less-or-equal-to.
    Lte,
}

impl Comparison {
    /// Returns a string representing this comparison in Lucene query formatting.
    pub fn as_lucene(&self) -> String {
        match self {
            Comparison::Gt => String::from(">"),
            Comparison::Lt => String::from("<"),
            Comparison::Gte => String::from(">="),
            Comparison::Lte => String::from("<="),
        }
    }
}

/// This enum represents the values we might be using in a comparison, whether
/// they are Strings, Numbers (currently only floating point numbers) or an
/// Unbounded comparison with no terminating value.
#[derive(Debug, Clone)]
pub enum ComparisonValue {
    Unbounded,
    String(String),
    Integer(i64),
    Float(f64),
}

impl std::fmt::Display for ComparisonValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "{}", s),
            Self::Integer(num) => write!(f, "{}", num),
            Self::Float(num) => write!(f, "{}", num),
            Self::Unbounded => write!(f, "*"),
        }
    }
}

impl ComparisonValue {
    /// Returns a string representing this value in Lucene query formatting
    pub fn to_lucene(&self) -> String {
        match self {
            Self::String(s) => QueryNode::lucene_escape(s),
            Self::Integer(num) => num.to_string(),
            Self::Float(num) => num.to_string(),
            Self::Unbounded => "*".to_string(),
        }
    }
}

impl<T: AsRef<str>> From<T> for ComparisonValue {
    fn from(s: T) -> Self {
        let v = escape_quotes(unescape(s.as_ref()));

        if v == "*" {
            ComparisonValue::Unbounded
        } else if let Ok(v) = v.parse::<i64>() {
            ComparisonValue::Integer(v)
        } else if let Ok(v) = v.parse::<f64>() {
            ComparisonValue::Float(v)
        } else {
            ComparisonValue::String(v)
        }
    }
}

/// This enum represents the tokens in a range, including "greater than (or equal to)"
/// for the left bracket, "less than (or equal to) in the right bracket, and range values.
#[derive(Debug, Clone)]
pub enum Range {
    Comparison(Comparison),
    Value(ComparisonValue),
}

/// This enum represents the AND or OR Boolean operations we might perform on QueryNodes.
#[derive(Debug, Copy, Clone)]
pub enum BooleanType {
    And,
    Or,
}

/// Builder structure to create Boolean QueryNodes.  Not strictly necessary,
/// however they're a bit more ergonomic to manipulate than reaching into
/// enums all the time.
pub struct BooleanBuilder {
    /// The type of Boolean operation this node will represent.
    oper: BooleanType,
    /// A list of QueryNodes involved in this boolean operation.
    nodes: Vec<QueryNode>,
}

impl BooleanBuilder {
    /// Create a BooleanBuilder to produce an AND-type Boolean QueryNode.
    pub fn and() -> Self {
        Self {
            oper: BooleanType::And,
            nodes: vec![],
        }
    }

    /// Create a BooleanBuilder to produce an OR-type Boolean QueryNode.
    pub fn or() -> Self {
        Self {
            oper: BooleanType::Or,
            nodes: vec![],
        }
    }

    /// Add a QueryNode to this boolean conjunction.
    pub fn add_node(&mut self, node: QueryNode) {
        self.nodes.push(node);
    }

    /// Consume this builder and output the finished QueryNode.
    pub fn build(self) -> QueryNode {
        let Self { oper, nodes } = self;
        QueryNode::Boolean { oper, nodes }
    }
}

/// QueryNodes represent specific search criteria to be enforced.
#[derive(Debug, Clone)]
pub enum QueryNode {
    /// Match all documents.
    MatchAllDocs,
    /// Match no documents.
    MatchNoDocs,
    /// Validate existence of an attribute within a document.
    AttributeExists { attr: String },
    /// Validate lack of an attribute within a document.
    AttributeMissing { attr: String },
    /// Match an attribute against a specific range of values.
    AttributeRange {
        attr: String,
        lower: ComparisonValue,
        lower_inclusive: bool,
        upper: ComparisonValue,
        upper_inclusive: bool,
    },
    /// Compare an attribute against a single value (greater/less than operations).
    AttributeComparison {
        attr: String,
        comparator: Comparison,
        value: ComparisonValue,
    },
    /// Search for an attribute that matches a specific term.
    AttributeTerm { attr: String, value: String },
    /// Search for an attribute that matches a specific quoted phrase.
    QuotedAttribute { attr: String, phrase: String },
    /// Search for an attribute whose value matches against a specific prefix.
    AttributePrefix { attr: String, prefix: String },
    /// Search for an attribute that matches a wildcard or glob string.
    AttributeWildcard { attr: String, wildcard: String },
    /// Container node denoting negation of the QueryNode within.
    NegatedNode { node: Box<QueryNode> },
    /// Container node for compound Boolean operations.
    Boolean {
        oper: BooleanType,
        nodes: Vec<QueryNode>,
    },
}

impl QueryNode {
    /// Returns a string representing this node in Lucene query formatting.
    pub fn to_lucene(&self) -> String {
        // TODO:  I'm using push_string here and there are more efficient string building methods if we care about performance here (we won't)
        match self {
            QueryNode::MatchAllDocs => String::from("*:*"),
            QueryNode::MatchNoDocs => String::from("-*:*"),
            QueryNode::AttributeExists { attr } => format!("_exists_:{}", attr),
            QueryNode::AttributeMissing { attr } => format!("_missing_:{}", attr),
            QueryNode::AttributeRange {
                attr,
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => {
                let lower_bracket = if *lower_inclusive { "[" } else { "{" };
                let upper_bracket = if *upper_inclusive { "]" } else { "}" };
                Self::is_default_attr(attr)
                    + &format!(
                        "{}{} TO {}{}",
                        lower_bracket,
                        lower.to_lucene(),
                        upper.to_lucene(),
                        upper_bracket
                    )
            }
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => {
                Self::is_default_attr(attr)
                    + &format!("{}{}", comparator.as_lucene(), value.to_lucene())
            }
            QueryNode::AttributeTerm { attr, value } => {
                Self::is_default_attr(attr) + &Self::lucene_escape(value)
            }
            QueryNode::QuotedAttribute { attr, phrase } => {
                Self::is_default_attr(attr) + &format!("\"{}\"", &Self::quoted_escape(phrase))
            }
            QueryNode::AttributePrefix { attr, prefix } => {
                Self::is_default_attr(attr) + &format!("{}*", &Self::lucene_escape(prefix))
            }
            QueryNode::AttributeWildcard { attr, wildcard } => {
                Self::is_default_attr(attr) + wildcard
            }
            QueryNode::NegatedNode { ref node } => {
                if matches!(
                    **node,
                    QueryNode::NegatedNode { .. } | QueryNode::Boolean { .. }
                ) {
                    format!("NOT ({})", node.to_lucene())
                } else {
                    format!("NOT {}", node.to_lucene())
                }
            }
            QueryNode::Boolean {
                oper: BooleanType::And,
                nodes,
                ..
            } => {
                if nodes.is_empty() {
                    return String::from("*:*");
                }
                let mut output = String::new();
                for n in nodes {
                    if !output.is_empty() {
                        // Put in ' AND ' if this isn't the first node we wrote
                        output.push_str(" AND ");
                    }
                    if let QueryNode::NegatedNode { node } = n {
                        output.push_str("NOT ");
                        let qstr = if let QueryNode::Boolean { .. } = **node {
                            format!("({})", node.to_lucene())
                        } else {
                            node.to_lucene()
                        };
                        output.push_str(&qstr);
                    } else {
                        let qstr = if let QueryNode::Boolean { .. } = n {
                            format!("({})", n.to_lucene())
                        } else {
                            n.to_lucene()
                        };
                        output.push_str(&qstr);
                    }
                }
                output
            }
            QueryNode::Boolean {
                oper: BooleanType::Or,
                nodes,
                ..
            } => {
                if nodes.is_empty() {
                    return String::from("-*:*");
                }
                let mut output = String::new();
                for n in nodes {
                    if !output.is_empty() {
                        output.push_str(" OR ");
                    }
                    let qstr = if let QueryNode::Boolean { .. } = n {
                        format!("({})", n.to_lucene())
                    } else {
                        n.to_lucene()
                    };
                    output.push_str(&qstr);
                }
                output
            }
        }
    }

    pub fn lucene_escape(input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        for c in input.chars() {
            // : + - = && || > < ! ( ) { } [ ] ^ " ~ * ? : \ /
            if matches!(
                c,
                ':' | '+'
                    | '-'
                    | '='
                    | '>'
                    | '<'
                    | '!'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '^'
                    | '"'
                    | '~'
                    | '*'
                    | '?'
                    | '\\'
                    | '/'
            ) {
                output.push('\\');
            }
            // TODO:  We're not catching '&&' and '||' but....does anyone do this?
            output.push(c);
        }
        output
    }

    fn quoted_escape(input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        for c in input.chars() {
            if matches!(c, '"' | '\\') {
                output.push('\\');
            }
            // TODO:  We're not catching '&&' and '||' but....does anyone do this?
            output.push(c);
        }
        output
    }

    fn is_default_attr(attr: &str) -> String {
        if attr == DEFAULT_FIELD {
            String::new()
        } else {
            format!("{}:", attr)
        }
    }
}

/// Enum representing Lucene's concept of whether a node should occur.
#[derive(Debug)]
pub enum LuceneOccur {
    Must,
    Should,
    MustNot,
}

#[derive(Debug)]
pub struct LuceneClause {
    pub occur: LuceneOccur,
    pub node: QueryNode,
}

/// Escapes surrounding `"` quotes when distinguishing between quoted terms isn't needed.
fn escape_quotes<T: AsRef<str>>(value: T) -> String {
    lazy_static::lazy_static! {
        static ref RE: Regex = Regex::new("^\"(.+)\"$").unwrap();
    }

    RE.replace_all(value.as_ref(), "$1").to_string()
}
