// src/state.rs
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use crate::graph::ESGraph;
use crate::server::ServerMessage;
use crate::db::Database;

pub struct AppState {
    pub graph: RwLock<ESGraph>,
    pub tx: broadcast::Sender<ServerMessage>,
    pub db: Option<Database>,
}

impl AppState {
    pub fn new(graph: ESGraph, db: Database) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
            db: Some(db),
        })
    }

    pub fn new_without_db(graph: ESGraph) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
            db: None,
        })
    }
}