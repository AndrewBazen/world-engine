use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::opt::auth::Root;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::graph::{ESGraph, ESNode, ESEdge, ESValue};

pub type Database = Surreal<Db>;

// serializable versions of your graph types for SurrealDB
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DbNode {
    pub key: String,
    pub namespace: String,
    pub node_type: String,
    pub id: String,
    pub props: HashMap<String, DbValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DbEdge {
    pub from_key: String,
    pub label: String,
    pub target_type: String,
    pub target_id: String,
    pub affinity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t", content = "v")]
pub enum DbValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

impl From<&ESValue> for DbValue {
    fn from(v: &ESValue) -> Self {
        match v {
            ESValue::Text(s)   => DbValue::Text(s.clone()),
            ESValue::Number(n) => DbValue::Number(*n),
            ESValue::Bool(b)   => DbValue::Bool(*b),
        }
    }
}

impl From<DbValue> for ESValue {
    fn from(v: DbValue) -> Self {
        match v {
            DbValue::Text(s)   => ESValue::Text(s),
            DbValue::Number(n) => ESValue::Number(n),
            DbValue::Bool(b)   => ESValue::Bool(b),
        }
    }
}

pub async fn connect() -> Result<Database, surrealdb::Error> {
    let db = Surreal::new::<RocksDb>("data/world.db").await?;
    db.use_ns("world_engine").use_db("world").await?;
    Ok(db)
}

pub async fn save_node(db: &Database, node: &ESNode, key: &str) -> Result<(), surrealdb::Error> {
    let db_node = DbNode {
        key: key.to_string(),
        namespace: node.namespace.clone(),
        node_type: node.node_type.clone(),
        id: node.id.clone(),
        props: node.props.iter()
            .map(|(k, v)| (k.clone(), DbValue::from(v)))
            .collect(),
    };

    db.upsert::<Option<DbNode>>(("nodes", key)).content(db_node).await?;

    // save edges separately
    // delete existing edges for this node first
    db.query("DELETE edge WHERE from_key = $key")
        .bind(("key", key.to_string()))
        .await?;

    for edge in &node.edges {
        let db_edge = DbEdge {
            from_key: key.to_string(),
            label: edge.label.clone(),
            target_type: edge.target_type.clone(),
            target_id: edge.target_id.clone(),
            affinity: edge.affinity,
        };
        db.create::<Option<DbEdge>>("edge").content(db_edge).await?;
    }

    Ok(())
}

pub async fn load_graph(db: &Database) -> Result<ESGraph, surrealdb::Error> {
    let mut graph = ESGraph::new();

    // load all nodes
    let db_nodes: Vec<DbNode> = db.select("nodes").await?;

    // load all edges
    let db_edges: Vec<DbEdge> = db.select("edge").await?;

    // build edge map by from_key
    let mut edge_map: HashMap<String, Vec<DbEdge>> = HashMap::new();
    for edge in db_edges {
        edge_map.entry(edge.from_key.clone()).or_default().push(edge);
    }

    // reconstruct ESGraph
    for db_node in db_nodes {
        let mut node = ESNode::new(&db_node.namespace, &db_node.node_type, &db_node.id);

        for (k, v) in db_node.props {
            node.props.insert(k, ESValue::from(v));
        }

        if let Some(edges) = edge_map.get(&db_node.key) {
            for e in edges {
                node.edges.push(ESEdge {
                    label: e.label.clone(),
                    target_type: e.target_type.clone(),
                    target_id: e.target_id.clone(),
                    affinity: e.affinity,
                });
            }
        }

        graph.insert(node);
    }

    Ok(graph)
}

pub async fn save_graph(db: &Database, graph: &ESGraph) -> Result<(), surrealdb::Error> {
    for (key, node) in &graph.nodes {
        save_node(db, node, key).await?;
    }
    Ok(())
}