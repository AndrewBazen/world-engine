// src/state.rs
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, RwLock};
use crate::graph::ESGraph;
use crate::server::ServerMessage;
use redb::Database;


pub struct AppState {
    pub graph: RwLock<ESGraph>,
    pub tx: broadcast::Sender<ServerMessage>,
    pub db: Option<Arc<Mutex<Database>>>,
}

impl AppState {
    pub fn new(graph: ESGraph, db: Database) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
            db: Some(Arc::new(Mutex::new(db))),
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