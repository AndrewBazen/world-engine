use axum::{
    extract::{State, WebSocketUpgrade},
    extract::ws::{WebSocket, Message},
    response::Response,
    routing::get,
    Router,
};
use tokio::sync::{broadcast, RwLock};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::StreamExt;
use tower_http::services::ServeDir;
use crate::{graph::ESGraph, state::AppState};

// messages the server broadcasts to all connected clients
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "snapshot")]
    Snapshot {
        nodes: Vec<NodeData>,
        edges: Vec<EdgeData>,
    },
    #[serde(rename = "signal_hop")]
    SignalHop {
        from: String,
        to: String,
        strength: f64,
        context: String,
        absorbed: bool,
        ambient: bool,
    },
    #[serde(rename = "node_update")]
    NodeUpdate {
        id: String,
        props: serde_json::Value,
    }
}

// serializable node for the browser
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeData {
    pub id: String,
    pub node_type: String,
    pub props: serde_json::Value,
}

// serializable edge for the browser
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EdgeData {
    pub source: String,
    pub target: String,
    pub label: String,
    pub affinity: f64,
}

pub async fn start(state: Arc<AppState>) {
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("visualizer"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Visualizer running at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    // send snapshot of current graph on connect
    let snapshot = build_snapshot(&state).await;
    if let Ok(msg) = serde_json::to_string(&snapshot) {
        let _ = socket.send(Message::Text(msg)).await;
    }

    //subscribe to broadcasts
    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            // broadcast from engine -> forward to this client
            Ok(msg) = rx.recv() => {
                if let Ok(text) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(text)).await.is_err() {
                        break;  // client disconnected
                    }
                }
            }
            // message from client -> handle trigger signal requests
            Some(Ok(Message::Text(text))) = socket.next() => {
                let state_clone = state.clone();
                let text_clone = text.clone();
                tokio::spawn(async move {
                    handle_client_message(&text_clone, &state_clone).await;
                });
            }
            else => break,
        }
    }
}

pub async fn build_snapshot(state: &Arc<AppState>) -> ServerMessage {
    let graph = state.graph.read().await;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for (key, node) in &graph.nodes {
        if !ESGraph::is_world_key(key) { continue; }
        nodes.push(NodeData {
            id: key.clone(),
            node_type: node.node_type.clone(),
            props: serde_json::to_value(&node.props).unwrap_or_default(),
        });
        for edge in &node.edges {
            let target_key = format!("{}:{}", edge.target_type, edge.target_id);
            if !ESGraph::is_world_key(&target_key) { continue; }
            edges.push(EdgeData {
                source: key.clone(),
                target: target_key,
                label: edge.label.clone(),
                affinity: edge.affinity,
            });
        }
    }

    ServerMessage::Snapshot { nodes, edges }
}

// handle trigger signal requests from thew browser
async fn handle_client_message(text: &str, state: &Arc<AppState>) {
    println!("handle_client_message: {}", text);
    #[derive(Deserialize)]
    #[serde(tag = "type")]
    enum ClientMessage {
        #[serde(rename = "trigger_signal")]
        TriggerSignal {
            origin_id: String,
            strength: f64,
            context: String,
        },
        #[serde(rename = "player_action")]
        PlayerAction {
            player_id: String,
            context: String,
            strength: f64,
        },
    }

    if let Ok(msg) = serde_json::from_str::<ClientMessage>(text) {
        match msg {
            ClientMessage::TriggerSignal { origin_id, strength, context } => {
                let signal = crate::signal::EventSignal::new(
                    &origin_id, strength, &context
                );
                crate::signal::propagate(state.clone(), signal).await;
            }
            ClientMessage::PlayerAction { player_id, context, strength } => {
                let action = crate::agent::PlayerAction {
                    player_id,
                    context,
                    strength,
                };
                if let Err(e) = crate::agent::agent_tick(state.clone(), action).await {
                    eprintln!("agent tick error: {}", e);
                }
            }
        }
    }
}