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

impl ESGraph {
    pub fn new() -> Self {
        ESGraph { nodes: HashMap::new() }
    }

    pub fn insert(&mut self, node: ESNode) {
        let key = format!("{}:{}", node.node_type, node.id);
        self.nodes.insert(key, node);
    }

    pub fn get(&self, node_type: &str, id: &str) -> Option<&ESNode> {
        let key = format!("{}:{}", node_type, id);
        self.nodes.get(&key)
    }
}

impl ESNode {
    pub fn new(node_type: &str, id: &str) -> Self {
        ESNode {
            node_type: node_type.to_string(),
            id: id.to_string(),
            props: HashMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn with_prop(mut self, key: &str, value: ESValue) -> Self {
        self.props.insert(key.to_string(), value);
        self
    }

    pub fn with_edge(mut self, label: &str, target_type: &str, target_id: &str) -> Self {
        self.edges.push(ESEdge {
            label: label.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
        });
        self
    }

    pub fn has_edge(&self, label: &str, target_type: &str, target_id: &str) -> bool {
        self.edges.iter().any(|e| {
            e.label == label && 
            e.target_type == target_type && 
            e.target_id == target_id
        })
    }

    pub fn edges_by_label(&self, label: &str) -> Vec<ESEdge> {
        self.edges.iter().filter(|e| e.label == label).map(|e| e.clone()).collect()
    }
}

impl ESEdge {
    pub fn new(label: &str, target_type: &str, target_id: &str) -> Self {
        ESEdge {
            label: label.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
        }
    }
}