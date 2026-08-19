use std::collections::HashMap;
use serde::{Serialize, Deserialize, Serializer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESNode {
    pub namespace: String,
    pub node_type: String,
    pub id: String,
    pub props: HashMap<String, ESValue>,
    pub edges: Vec<ESEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESEdge {
    pub label: String,
    pub target_namespace: String,
    pub target_type: String,
    pub target_id: String,
    pub affinity: f64,
    pub remove: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ESValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESGraph {
    pub nodes: HashMap<String, ESNode>,
}

impl ESGraph {
    pub fn new() -> Self {
        ESGraph { nodes: HashMap::new() }
    }

    pub fn make_key(namespace: &str, node_type: &str, id: &str) -> String {
        if namespace == "world" || namespace.is_empty() {
            format!("{}:{}", node_type, id)
        } else {
            format!("{}/{}:{}", namespace, node_type, id)
        }
    }

    pub fn insert(&mut self, node: ESNode) {
        let key = Self::make_key(&node.namespace, &node.node_type, &node.id);
        self.nodes.insert(key, node);
    }

    /// Insert, or fold into the node already at this key.
    ///
    /// Models declare the same node several times in one patch constantly. With
    /// a plain `insert` the last declaration replaced the earlier ones outright,
    /// so a block's properties vanished with nothing reported.
    pub fn merge_node(&mut self, node: ESNode) {
        let key = Self::make_key(&node.namespace, &node.node_type, &node.id);
        match self.nodes.get_mut(&key) {
            Some(existing) => {
                for (k, v) in node.props {
                    existing.props.insert(k, v);
                }
                for edge in node.edges {
                    let duplicate = existing.edges.iter().any(|e| {
                        e.label == edge.label
                            && e.target_type == edge.target_type
                            && e.target_id == edge.target_id
                    });
                    if !duplicate {
                        existing.edges.push(edge);
                    }
                }
            }
            None => {
                self.nodes.insert(key, node);
            }
        }
    }

    pub fn get(&self, namespace: &str, node_type: &str, id: &str) -> Option<&ESNode> {
        let key = Self::make_key(namespace, node_type, id);
        self.nodes.get(&key)
    }

    pub fn get_mut(&mut self, namespace: &str, node_type: &str, id: &str) -> Option<&mut ESNode> {
        let key = Self::make_key(namespace, node_type, id);
        self.nodes.get_mut(&key)
    }

    pub fn get_by_key(&self, key: &str) -> Option<&ESNode> {
        self.nodes.get(key)
    }

    pub fn get_mut_by_key(&mut self, key: &str) -> Option<&mut ESNode> {
        self.nodes.get_mut(key)
    }

    pub fn is_world_key(key: &str) -> bool {
        !key.contains('/')
    }
}

impl ESNode {
    // constructor

    pub fn new(namespace: &str, node_type: &str, id: &str) -> Self {
        ESNode {
            namespace: namespace.to_string(),
            node_type: node_type.to_string(),
            id: id.to_string(),
            props: HashMap::new(),
            edges: Vec::new(),
        }
    }

    // property methods

    pub fn with_prop(mut self, key: &str, value: ESValue) -> Self {
        self.props.insert(key.to_string(), value);
        self
    }

    // edge methods

    pub fn with_edge(mut self, label: &str, target_type: &str, target_id: &str) -> Self {
        self.edges.push(ESEdge::new(label, target_type, target_id));
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

    // ── Property accessors ─────────────────────────────────────

    pub fn get_number(&self, key: &str) -> Option<f64> {
        match self.props.get(key) {
            Some(ESValue::Number(n)) => Some(*n),
            _ => None,
        }
    }
    
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.props.get(key) {
            Some(ESValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }
}

impl ESEdge {
    pub fn new(label: &str, target_type: &str, target_id: &str) -> Self {
        ESEdge {
            label: label.to_string(),
            target_namespace: "world".to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            affinity: 1.0,
            remove: false,
        }
    }
}

impl Serialize for ESValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer {
                match self {
                    ESValue::Text(s)   => serializer.serialize_str(s),
                    ESValue::Number(n)    => serializer.serialize_f64(*n),
                    ESValue::Bool(b)     => serializer.serialize_bool(*b),
                }
    }
}
