use std::sync::atomic::AtomicU64;
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
    pub turn: AtomicU64,
}

impl AppState {
    pub fn new(graph: ESGraph, db: Database, turn: u64) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
            db: Some(Arc::new(Mutex::new(db))),
            turn: AtomicU64::new(turn),
        })
    }

    pub fn new_without_db(graph: ESGraph) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
            db: None,
            turn: AtomicU64::new(0),
        })
    }
}