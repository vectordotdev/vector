use crate::grammar::DEFAULT_FIELD;

/// This enum represents value comparisons that Queries might perform
#[derive(Debug, Copy, Clone)]
pub enum Comparison {
    /// Greater than
    GT,
    /// Less than
    LT,
    /// Greater-or-equal-to
    GTE,
    /// Less-or-equal-to
    LTE,
}

impl Comparison {
    /// Returns a string representing this comparison in Lucene query formatting
    pub fn to_lucene(&self) -> String {
        match self {
            Comparison::GT => String::from(">"),
            Comparison::LT => String::from("<"),
            Comparison::GTE => String::from(">="),
            Comparison::LTE => String::from("<="),
        }
    }
}

/// This enum represents the values we might be using in a comparison, whether
/// they are Strings, Numbers (currently only floating point numbers) or an
/// Unbounded comparison with no terminating value
#[derive(Debug, Clone)]
pub enum ComparisonValue {
    Unbounded,
    String(String),
    Numeric(f64),
}

impl ComparisonValue {
    /// Returns a string representing this value in Lucene query formatting
    pub fn to_lucene(&self) -> String {
        match self {
            Self::String(s) => QueryNode::lucene_escape(s),
            Self::Numeric(num) => num.to_string(),
            Self::Unbounded => "*".to_string(),
        }
    }
}

/// This enum represents the AND or OR Boolean operations we might perform on QueryNodes
#[derive(Debug, Copy, Clone)]
pub enum BooleanType {
    And,
    Or,
}

/// Builder structure to create Boolean QueryNodes.  Not strictly necessary,
/// however they're a bit more ergonomic to manipulate than reaching into
/// enums all the time
pub struct BooleanBuilder {
    /// The type of Boolean operation this node will represent
    oper: BooleanType,
    /// A list of QueryNodes involved in this boolean operation
    nodes: Vec<QueryNode>,
}

impl BooleanBuilder {
    /// Create a BooleanBuilder to produce an AND-type Boolean QueryNode
    pub fn and() -> Self {
        Self {
            oper: BooleanType::And,
            nodes: vec![],
        }
    }

    /// Create a BooleanBuilder to produce an OR-type Boolean QueryNode
    pub fn or() -> Self {
        Self {
            oper: BooleanType::Or,
            nodes: vec![],
        }
    }

    /// Add a QueryNode to this boolean conjunction
    pub fn add_node(&mut self, node: QueryNode) {
        self.nodes.push(node);
    }

    /// Consume this builder and output the finished QueryNode
    pub fn build(self) -> QueryNode {
        let Self { oper, nodes } = self;
        QueryNode::Boolean { oper, nodes }
    }
}

/// QueryNodes represent specific search criteria to be enforced
#[derive(Debug, Clone)]
pub enum QueryNode {
    /// Match all documents
    MatchAllDocs,
    /// Match no documents
    MatchNoDocs,
    /// Validate existance of an attribute within a document
    AttributeExists { attr: String },
    /// Validate lack of an attribute within a document
    AttributeMissing { attr: String },
    /// Match an attribute against a specific range of values
    AttributeRange {
        attr: String,
        lower: ComparisonValue,
        lower_inclusive: bool,
        upper: ComparisonValue,
        upper_inclusive: bool,
    },
    /// Compare an attribute against a single value (greater/less than operations)
    AttributeComparison {
        attr: String,
        comparator: Comparison,
        value: ComparisonValue,
    },
    /// Search for an attribute that matches a specific term
    AttributeTerm { attr: String, value: String },
    /// Search for an attribute that matches a specific quoted phrase
    QuotedAttribute { attr: String, phrase: String },
    /// Search for an attribute whose value matches against a specific prefix
    AttributePrefix { attr: String, prefix: String },
    /// Search for an attribute that matches a wildcard or glob string
    AttributeWildcard { attr: String, wildcard: String },
    /// Container node denoting negation of the QueryNode within
    NegatedNode { node: Box<QueryNode> },
    /// Container node for compound Boolean operations
    Boolean {
        oper: BooleanType,
        nodes: Vec<QueryNode>,
    },
}

impl QueryNode {
    /// Returns a string representing this node in Lucene query formatting
    pub fn to_lucene(&self) -> String {
        // TODO:  I'm using push_string here and there are more efficient string building methods if we care about performance here (we won't)
        match self {
            QueryNode::MatchAllDocs => return String::from("*:*"),
            QueryNode::MatchNoDocs => return String::from("-*:*"),
            QueryNode::AttributeExists { attr } => return format!("_exists_:{}", attr),
            QueryNode::AttributeMissing { attr } => return format!("_missing_:{}", attr),
            QueryNode::AttributeRange {
                attr,
                lower,
                lower_inclusive,
                upper,
                upper_inclusive,
            } => {
                let lower_bracket = if *lower_inclusive { "[" } else { "{" };
                let upper_bracket = if *upper_inclusive { "]" } else { "}" };
                return Self::is_default_attr(attr)
                    + &format!(
                        "{}{} TO {}{}",
                        lower_bracket,
                        lower.to_lucene(),
                        upper.to_lucene(),
                        upper_bracket
                    );
            }
            QueryNode::AttributeComparison {
                attr,
                comparator,
                value,
            } => {
                return Self::is_default_attr(attr)
                    + &format!("{}{}", comparator.to_lucene(), value.to_lucene());
            }
            QueryNode::AttributeTerm { attr, value } => {
                return Self::is_default_attr(attr) + &Self::lucene_escape(value)
            }
            QueryNode::QuotedAttribute { attr, phrase } => {
                return Self::is_default_attr(attr)
                    + &format!("\"{}\"", &Self::quoted_escape(phrase))
            }
            QueryNode::AttributePrefix { attr, prefix } => {
                return Self::is_default_attr(attr) + &format!("{}*", &Self::lucene_escape(prefix))
            }
            QueryNode::AttributeWildcard { attr, wildcard } => {
                return Self::is_default_attr(attr) + &format!("{}", wildcard)
            }
            QueryNode::NegatedNode { ref node } => {
                if matches!(
                    **node,
                    QueryNode::NegatedNode { .. } | QueryNode::Boolean { .. }
                ) {
                    return format!("NOT ({})", node.to_lucene());
                } else {
                    return format!("NOT {}", node.to_lucene());
                }
            }
            QueryNode::Boolean {
                oper: BooleanType::And,
                nodes,
                ..
            } => {
                if nodes.len() == 0 {
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
                if nodes.len() == 0 {
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

/// Enum representing Lucene's concept of qu
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
