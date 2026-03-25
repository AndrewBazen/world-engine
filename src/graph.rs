use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::signal::EventSignal;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESNode {
    pub node_type: String,
    pub id: String,
    pub props: HashMap<String, ESValue>,
    pub edges: Vec<ESEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESEdge {
    pub label: String,
    pub target_type: String,
    pub target_id: String,
    pub affinity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn insert(&mut self, node: ESNode) {
        let key = format!("{}:{}", node.node_type, node.id);
        self.nodes.insert(key, node);
    }

    pub fn get(&self, node_type: &str, id: &str) -> Option<&ESNode> {
        let key = format!("{}:{}", node_type, id);
        self.nodes.get(&key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut ESNode> {
        self.nodes.get_mut(key)
    }

    pub fn get_by_key(&self, key: &str) -> Option<&ESNode> {
        self.nodes.get(key)
    }
    
    pub fn get_mut_by_key(&mut self, key: &str) -> Option<&mut ESNode> {
        self.nodes.get_mut(key)
    }
}

impl ESNode {
    // constructor

    pub fn new(node_type: &str, id: &str) -> Self {
        ESNode {
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
        self.edges.push(ESEdge {
            label: label.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            affinity: 1.0,
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
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            affinity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_node_creation() {
        let mut graph = ESGraph::new();
        
        let player = ESNode::new("player", "andrew")
            .with_prop("strength", ESValue::Number(14.0))
            .with_prop("class", ESValue::Text("Compensated Anarchist".to_string()))
            .with_prop("alive", ESValue::Bool(true))
            .with_prop("intelligence", ESValue::Number(14.0))
            .with_edge("playing", "session", "s1");

        graph.insert(player);
        
        // can we get it back?
        let retrieved = graph.get("player", "andrew");
        assert!(retrieved.is_some());

        let node = retrieved.unwrap();
        assert_eq!(node.id, "andrew");
        assert_eq!(node.node_type, "player");

        // check prop types
        let strength = node.props.get("strength");
        let class = node.props.get("class");
        let alive = node.props.get("alive");
        assert!(matches!(strength, Some(ESValue::Number(v)) if *v == 14.0));
        assert!(matches!(class, Some(ESValue::Text(s)) if s == "Compensated Anarchist"));
        assert!(matches!(alive, Some(ESValue::Bool(b)) if *b == true));
        

        // check an edge
        assert_eq!(node.edges.len(), 1);
        assert_eq!(node.edges[0].label, "playing");
        assert_eq!(node.edges[0].target_type, "session");
        assert_eq!(node.edges[0].target_id, "s1");
    }

    #[test]
    fn test_missing_node_returns_none() {
        let graph = ESGraph::new();
        assert!(graph.get("player", "nobody").is_none());
    }
}