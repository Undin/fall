use fall_tree::{NodeType};
use lex::Token;

use tree_builder::{Node, TokenSequence, NodeFactory};

pub struct Parser<'r> {
    node_types: &'r [NodeType],
    rules: &'r [SynRule],
}

#[derive(Serialize, Deserialize)]
pub struct SynRule {
    pub ty: Option<usize>,
    pub body: Expr,
}

#[derive(Serialize, Deserialize)]
pub enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>, Option<usize>),
    Rule(usize),
    Token(usize),
    Rep(Box<Expr>),
    Opt(Box<Expr>),
    Not(Vec<usize>),
    Ahead(Vec<usize>),
    Eof,
    Layer(Box<Expr>, Box<Expr>),
    SkipUntil(Vec<usize>),
}

impl<'r> Parser<'r> {
    pub fn new(node_types: &'r [NodeType], rules: &'r [SynRule]) -> Parser<'r> {
        Parser { node_types, rules: rules }
    }

    pub fn parse(&self, tokens: TokenSequence, nf: &mut NodeFactory) -> Node {
        let (mut file_node, mut leftover) = self.parse_exp(&Expr::Rule(0), tokens, nf).unwrap_or_else(|| {
            let ty = self.node_type(self.rules[0].ty
                .expect("First rule must be public"));
            (nf.create_composite_node(Some(ty)), tokens)
        });
        let mut error = nf.create_error_node();
        let mut skipped = false;
        while leftover.current().is_some() {
            skipped = true;
            let p = nf.create_leaf_node(leftover);
            leftover = p.1;
            error.push_child(p.0)
        }
        if skipped {
            file_node.push_child(error)
        }
        file_node
    }

    fn parse_exp<'t>(&self, expr: &Expr, tokens: TokenSequence<'t>, nf: &mut NodeFactory)
                     -> Option<(Node, TokenSequence<'t>)> {
        match *expr {
            Expr::Or(ref parts) => {
                for p in parts.iter() {
                    if let Some(result) = self.parse_exp(p, tokens, nf) {
                        return Some(result)
                    }
                }
                None
            }

            Expr::And(ref parts, commit) => {
                let mut node = nf.create_composite_node(None);
                let commit = commit.unwrap_or(parts.len());
                let mut tokens = tokens;
                for (i, p) in parts.iter().enumerate() {
                    if let Some((n, ts)) = self.parse_exp(p, tokens, nf) {
                        tokens = ts;
                        node.push_child(n);
                    } else {
                        if i < commit {
                            return None
                        }
                        let error_node = nf.create_error_node();
                        node.push_child(error_node);
                        break
                    }
                }
                Some((node, tokens))
            }

            Expr::Rule(id) => {
                let rule = &self.rules[id];
                let ty = rule.ty.map(|ty| self.node_type(ty));
                if let Some((node, ts)) = self.parse_exp(&rule.body, tokens, nf) {
                    let mut result = nf.create_composite_node(ty);
                    result.push_child(node);
                    Some((result, ts))
                } else {
                    None
                }
            }

            Expr::Token(ty) => {
                if let Some(current) = tokens.current() {
                    if self.token_set_contains(&[ty], current) {
                        return Some(nf.create_leaf_node(tokens))
                    }
                }
                None
            }

            Expr::Opt(ref body) => {
                self.parse_exp(&*body, tokens, nf).or_else(|| {
                    Some((nf.create_composite_node(None), tokens))
                })
            }

            Expr::Not(ref ts) => {
                if let Some(current) = tokens.current() {
                    if !self.token_set_contains(ts, current) {
                        return Some(nf.create_leaf_node(tokens))
                    }
                }
                None
            }

            Expr::Ahead(ref ts) => {
                if let Some(current) = tokens.current() {
                    if self.token_set_contains(ts, current) {
                        return Some((nf.create_composite_node(None), tokens))
                    }
                }
                None
            }

            Expr::Eof => if tokens.current().is_none() {
                Some((nf.create_composite_node(None), tokens))
            } else {
                None
            },

            Expr::Layer(ref l, ref e) => {
                if let Some((layer_node, rest)) = self.parse_exp(l, tokens, nf) {
                    let mut result = nf.create_composite_node(None);
                    let layer = tokens.tokens_of_node(&layer_node);
                    if let Some((layer_contents, mut leftovers)) = self.parse_exp(e, layer, nf) {
                        result.push_child(layer_contents);
                        if leftovers.current().is_some() {
                            let mut error = nf.create_error_node();
                            while leftovers.current().is_some() {
                                let p = nf.create_leaf_node(leftovers);
                                error.push_child(p.0);
                                leftovers = p.1;
                            }
                            result.push_child(error)
                        }
                    };
                    return Some((result, rest));
                };
                None
            }

            Expr::Rep(ref body) => {
                let mut node = nf.create_composite_node(None);
                let mut tokens = tokens;
                loop {
                    if let Some((n, t)) = self.parse_exp(body, tokens, nf) {
                        node.push_child(n);
                        tokens = t;
                    } else {
                        break;
                    }
                }
                Some((node, tokens))
            }

            Expr::SkipUntil(ref ts) => {
                let mut result = nf.create_error_node();
                let mut skipped = false;
                let mut tokens = tokens;
                loop {
                    if let Some(t) = tokens.current() {
                        if self.token_set_contains(ts, t) {
                            break;
                        } else {
                            skipped = true;
                            let p = nf.create_leaf_node(tokens);
                            result.push_child(p.0);
                            tokens = p.1;
                        }
                    } else {
                        break
                    }
                }

                if !skipped {
                    result = nf.create_composite_node(None);
                }

                Some((result, tokens))
            }
        }
    }

    fn token_set_contains(&self, ts: &[usize], token: Token) -> bool {
        ts.iter().any(|&t| self.node_type(t) == token.ty)
    }

    fn node_type(&self, idx: usize) -> NodeType {
        self.node_types[idx]
    }
}

