// src/state.rs
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use crate::graph::ESGraph;
use crate::server::ServerMessage;

pub struct AppState {
    pub graph: RwLock<ESGraph>,
    pub tx: broadcast::Sender<ServerMessage>,
}

impl AppState {
    pub fn new(graph: ESGraph) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(AppState {
            graph: RwLock::new(graph),
            tx,
        })
    }
}