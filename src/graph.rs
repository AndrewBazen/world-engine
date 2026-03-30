use std::collections::HashMap;
use serde::{Serialize, Deserialize, Serializer};
use crate::signal::EventSignal;

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

    // Signal methods

    pub fn should_absorb(&self, signal: &EventSignal, strength: f64) -> bool {
        let threshold = self.get_number("threshold").unwrap_or(0.5);
        strength >= threshold
    }
    
    pub fn absorb(&mut self, signal: &EventSignal, strength: f64) {
        self.props.insert(
            "last_signal_context".to_string(),
            ESValue::Text(signal.context.clone())
        );
        self.props.insert(
            "last_signal_strength".to_string(),
            ESValue::Number(strength)
        );
        let current = self.get_number("activation").unwrap_or(0.0);
        self.props.insert(
            "activation".to_string(),
            ESValue::Number((current + strength).min(1.0))
        );

    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_node_creation() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_prop("courage", ESValue::Number(14.0))
            .with_prop("name", ESValue::Text("Andrew".to_string()))
            .with_edge("playing", "session", "s1");

        graph.insert(player);

        let retrieved = graph.get("world", "player", "andrew");
        assert!(retrieved.is_some());

        let node = retrieved.unwrap();
        assert_eq!(node.id, "andrew");
        assert_eq!(node.node_type, "player");
        assert_eq!(node.namespace, "world");

        let courage = node.props.get("courage");
        assert!(matches!(courage, Some(ESValue::Number(v)) if *v == 14.0));

        assert_eq!(node.edges.len(), 1);
        assert_eq!(node.edges[0].label, "playing");
        assert_eq!(node.edges[0].target_id, "s1");
    }

    #[test]
    fn test_missing_node_returns_none() {
        let graph = ESGraph::new();
        assert!(graph.get("world", "player", "nobody").is_none());
    }

    #[test]
    fn test_namespace_key_format() {
        // world nodes use simple key
        assert_eq!(ESGraph::make_key("world", "player", "andrew"), "player:andrew");
        
        // private nodes use namespace prefix
        assert_eq!(
            ESGraph::make_key("inventory/andrew", "item", "sword"),
            "inventory/andrew/item:sword"
        );
    }

    #[test]
    fn test_is_world_key() {
        assert!(ESGraph::is_world_key("player:andrew"));
        assert!(ESGraph::is_world_key("npc:guard"));
        assert!(!ESGraph::is_world_key("inventory/andrew/item:sword"));
        assert!(!ESGraph::is_world_key("memory/guard/event:theft"));
    }
}