use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ESNode {
    pub node_type: String,
    pub id: String,
    pub props: HashMap<String, ESValue>,
    pub edges: Vec<ESEdge>,
}

#[derive(Debug, Clone)]
pub struct ESEdge {
    pub label: String,
    pub target_type: String,
    pub target_id: String,
}

#[derive(Debug, Clone)]
pub enum ESValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct ESGraph {
    pub nodes: HashMap<String, ESNode>,
}