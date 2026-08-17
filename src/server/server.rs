use axum::{
    Router, extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}}, response::Response, routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::StreamExt;
use tokio::sync::broadcast;
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
        /// The signal passed through a non-perceiving node (an item, a place,
        /// a faction) on its way somewhere else. Not an absorb, not an ignore.
        #[serde(default)]
        transit: bool,
        /// Ring index from the origin. The client staggers its animation by
        /// this rather than the engine sleeping between rings.
        #[serde(default)]
        hop: u32,
    },
    #[serde(rename = "node_update")]
    NodeUpdate {
        id: String,
        props: serde_json::Value,
    },
    #[serde(rename = "node_detail")]
    NodeDetail {
        center: String,
        nodes: Vec<NodeData>,
        edges: Vec<EdgeData>,
    },
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
        let _ = socket.send(Message::Text(msg.into())).await;
    }

    //subscribe to broadcasts
    let mut rx = state.tx.subscribe();

    // Replies meant for THIS client only. Node detail used to go out over the
    // broadcast channel, so one person clicking a node opened the inspector
    // for everybody.
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMessage>();

    loop {
        tokio::select! {
            // broadcast from engine -> forward to this client
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(msg) => {
                        if let Ok(text) = serde_json::to_string(&msg) {
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                break;  // client disconnected
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Slow client. Resync with a fresh snapshot rather than
                        // dropping the connection.
                        eprintln!("client lagged {} messages, resyncing", n);
                        let snapshot = build_snapshot(&state).await;
                        if let Ok(text) = serde_json::to_string(&snapshot) {
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // direct reply to this client
            Some(msg) = reply_rx.recv() => {
                if let Ok(text) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
            // message from client -> handle trigger signal requests
            Some(Ok(Message::Text(text))) = socket.next() => {
                let state_clone = state.clone();
                let text_clone = text.clone();
                let reply = reply_tx.clone();
                tokio::spawn(async move {
                    handle_client_message(&text_clone, &state_clone, reply).await;
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
            if !graph.nodes.contains_key(&target_key) || !ESGraph::is_world_key(&target_key) { continue; }
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
async fn handle_client_message(
    text: &str,
    state: &Arc<AppState>,
    reply: tokio::sync::mpsc::UnboundedSender<ServerMessage>,
) {
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
        #[serde(rename = "node_detail")]
        NodeDetail {
            node_id: String,
        },
    }

    if let Ok(msg) = serde_json::from_str::<ClientMessage>(text) {
        match msg {

            ClientMessage::TriggerSignal { origin_id, strength, context } => {
                let signal = crate::signal::EventSignal::new(
                    &origin_id, strength, &context
                );
                let (_absorbed, _visited) = crate::signal::propagate(state.clone(), signal).await;
            }
            ClientMessage::PlayerAction { player_id, context, strength } => {
                let action = crate::agent::PlayerAction {
                    player_id,
                    context,
                    strength,
                };
                if let Err(e) = crate::agent::handle_player_input(state.clone(), action).await {
                    eprintln!("agent tick error: {}", e);
                }
            }
            ClientMessage::NodeDetail { node_id } => {
                let detail = build_node_detail(state, &node_id).await;
                let _ = reply.send(detail);
            }
        }
    }
}

pub async fn build_node_detail(state: &Arc<AppState>, node_id: &str) -> ServerMessage {
    let graph = state.graph.read().await;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let name = node_id.split(':').nth(1).unwrap_or(node_id);
    let node_type = node_id.split(':').next().unwrap_or("");

    // always include the center node itself
    if let Some(node) = graph.nodes.get(node_id) {
        nodes.push(NodeData {
            id: node_id.to_string(),
            node_type: node.node_type.clone(),
            props: serde_json::to_value(&node.props).unwrap_or_default(),
        });
    }

    // determine which namespaces to search
    let mut prefixes = vec![
        format!("stats/{}/", name),
        format!("memory/{}/", name),
    ];

    if node_type == "player" {
        prefixes.push(format!("inventory/{}/", name));
        prefixes.push(format!("equipped/{}/", name));
        prefixes.push(format!("abilities/{}/", name));
        prefixes.push(format!("quests/{}/", name));
    }

    // collect all nodes matching the prefixes
    for (key, node) in &graph.nodes {
        if prefixes.iter().any(|p| key.starts_with(p)) {
            nodes.push(NodeData {
                id: key.clone(),
                node_type: node.node_type.clone(),
                props: serde_json::to_value(&node.props).unwrap_or_default(),
            });
            for edge in &node.edges {
                let target_key = format!("{}:{}", edge.target_type, edge.target_id);
                edges.push(EdgeData {
                    source: key.clone(),
                    target: target_key,
                    label: edge.label.clone(),
                    affinity: edge.affinity,
                });
            }
        }
    }

    // also include edges from the center node
    if let Some(node) = graph.nodes.get(node_id) {
        for edge in &node.edges {
            let target_key = format!("{}:{}", edge.target_type, edge.target_id);
            edges.push(EdgeData {
                source: node_id.to_string(),
                target: target_key,
                label: edge.label.clone(),
                affinity: edge.affinity,
            });
        }
    }

    ServerMessage::NodeDetail {
        center: node_id.to_string(),
        nodes,
        edges,
    }
}