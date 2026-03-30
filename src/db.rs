use redb::{Database, TableDefinition, ReadableTable};
use crate::graph::{ESGraph, ESNode, ESEdge, ESValue};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

const NODES: TableDefinition<&str, &str> = TableDefinition::new("nodes");
const EDGES: TableDefinition<&str, &str> = TableDefinition::new("edges");

#[derive(Serialize, Deserialize)]
struct StoredNode {
    namespace: String,
    node_type: String,
    id: String,
    props: HashMap<String, StoredValue>,
}

fn default_world() -> String { "world".to_string() }

#[derive(Serialize, Deserialize)]
struct StoredEdge {
    label: String,
    #[serde(default = "default_world")]
    target_namespace: String,
    target_type: String,
    target_id: String,
    affinity: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
enum StoredValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

impl From<&ESValue> for StoredValue {
    fn from(v: &ESValue) -> Self {
        match v {
            ESValue::Text(s)   => StoredValue::Text(s.clone()),
            ESValue::Number(n) => StoredValue::Number(*n),
            ESValue::Bool(b)   => StoredValue::Bool(*b),
        }
    }
}

impl From<StoredValue> for ESValue {
    fn from(v: StoredValue) -> Self {
        match v {
            StoredValue::Text(s)   => ESValue::Text(s),
            StoredValue::Number(n) => ESValue::Number(n),
            StoredValue::Bool(b)   => ESValue::Bool(b),
        }
    }
}

pub fn connect() -> Result<Database, redb::Error> {
    std::fs::create_dir_all("data").ok();
    let db = Database::create("data/world.db")?;
    
    // ensure tables exist
    let write_txn = db.begin_write()?;
    write_txn.open_table(NODES)?;
    write_txn.open_table(EDGES)?;
    write_txn.commit()?;
    
    Ok(db)
}

pub fn save_node(db: &Database, key: &str, node: &ESNode) -> Result<(), redb::Error> {
    let stored = StoredNode {
        namespace: node.namespace.clone(),
        node_type: node.node_type.clone(),
        id: node.id.clone(),
        props: node.props.iter()
            .map(|(k, v)| (k.clone(), StoredValue::from(v)))
            .collect(),
    };

    let node_json = serde_json::to_string(&stored).unwrap();

    // save edges as a separate entry
    let stored_edges: Vec<StoredEdge> = node.edges.iter().map(|e| StoredEdge {
        label: e.label.clone(),
        target_namespace: e.target_namespace.clone(),
        target_type: e.target_type.clone(),
        target_id: e.target_id.clone(),
        affinity: e.affinity,
    }).collect();
    let edges_json = serde_json::to_string(&stored_edges).unwrap();

    let write_txn = db.begin_write()?;
    {
        let mut node_table = write_txn.open_table(NODES)?;
        node_table.insert(key, node_json.as_str())?;

        let mut edge_table = write_txn.open_table(EDGES)?;
        edge_table.insert(key, edges_json.as_str())?;
    }
    write_txn.commit()?;

    Ok(())
}

pub fn save_graph(db: &Database, graph: &ESGraph) -> Result<(), redb::Error> {
    let write_txn = db.begin_write()?;
    {
        let mut node_table = write_txn.open_table(NODES)?;
        let mut edge_table = write_txn.open_table(EDGES)?;

        for (key, node) in &graph.nodes {
            let stored = StoredNode {
                namespace: node.namespace.clone(),
                node_type: node.node_type.clone(),
                id: node.id.clone(),
                props: node.props.iter()
                    .map(|(k, v)| (k.clone(), StoredValue::from(v)))
                    .collect(),
            };
            let node_json = serde_json::to_string(&stored).unwrap();
            node_table.insert(key.as_str(), node_json.as_str())?;

            let stored_edges: Vec<StoredEdge> = node.edges.iter().map(|e| StoredEdge {
                label: e.label.clone(),
                target_namespace: e.target_namespace.clone(),
                target_type: e.target_type.clone(),
                target_id: e.target_id.clone(),
                affinity: e.affinity,
            }).collect();
            let edges_json = serde_json::to_string(&stored_edges).unwrap();
            edge_table.insert(key.as_str(), edges_json.as_str())?;
        }
    }
    write_txn.commit()?;
    Ok(())
}

pub fn load_graph(db: &Database) -> Result<ESGraph, redb::Error> {
    let mut graph = ESGraph::new();

    let read_txn = db.begin_read()?;
    let node_table = read_txn.open_table(NODES)?;
    let edge_table = read_txn.open_table(EDGES)?;

    for entry in node_table.iter()? {
        let (key, value) = entry?;
        let key_str = key.value();
        let json = value.value();

        if let Ok(stored) = serde_json::from_str::<StoredNode>(json) {
            let mut node = ESNode::new(
                &stored.namespace,
                &stored.node_type,
                &stored.id,
            );

            for (k, v) in stored.props {
                node.props.insert(k, ESValue::from(v));
            }

            // load edges
            if let Ok(edge_entry) = edge_table.get(key_str) {
                if let Some(edge_value) = edge_entry {
                    if let Ok(edges) = serde_json::from_str::<Vec<StoredEdge>>(edge_value.value()) {
                        for e in edges {
                            node.edges.push(ESEdge {
                                label: e.label,
                                target_namespace: e.target_namespace,
                                target_type: e.target_type,
                                target_id: e.target_id,
                                affinity: e.affinity,
                                remove: false,
                            });
                        }
                    }
                }
            }

            graph.nodes.insert(key_str.to_string(), node);
        }
    }

    Ok(graph)
}