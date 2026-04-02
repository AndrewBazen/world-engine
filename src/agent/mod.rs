mod player;
mod npc;
mod handlers;

use std::sync::Arc;
use crate::graph::{ESGraph};
use crate::state::AppState;

pub const VERBOSE: bool = false;

pub struct PlayerAction {
    pub player_id: String,
    pub context: String,
    pub strength: f64,
}

pub fn format_value(v: &crate::graph::ESValue) -> String {
    match v {
        crate::graph::ESValue::Text(s)   => s.clone(),
        crate::graph::ESValue::Number(n) => n.to_string(),
        crate::graph::ESValue::Bool(b)   => b.to_string(),
    }
}

pub fn merge_patch(world: &mut ESGraph, patch: ESGraph, allowed_namespaces: &[&str]) {
    for (key, patch_node) in patch.nodes {
        if key.starts_with("remove:") { continue; }

        let namespace = &patch_node.namespace;
        let is_allowed = allowed_namespaces.iter().any(|ns| {
            namespace == *ns || namespace.starts_with(ns)
        });

        if !is_allowed {
            println!("rejected write to namespace: {}", namespace);
            continue;
        }

        if let Some(existing) = world.nodes.get_mut(&key) {
            for (k, v) in patch_node.props {
                existing.props.insert(k, v);
            }
            for edge in patch_node.edges {
                let already_exists = existing.edges.iter().any(|e| {
                    e.label == edge.label
                    && e.target_namespace == edge.target_namespace
                    && e.target_type == edge.target_type
                    && e.target_id == edge.target_id
                });
                if !already_exists {
                    existing.edges.push(edge);
                }
            }
        } else {
            world.nodes.insert(key, patch_node);
        }
    }
}

pub async fn handle_player_input(
    state: Arc<AppState>,
    action: PlayerAction,
) -> Result<(), String> {
    let location = {
        let graph = state.graph.read().await;
        match graph.nodes.get(&action.player_id) {
            Some(n) => match n.props.get("location") {
                Some(crate::graph::ESValue::Text(l)) => l.clone(),
                _ => "unknown".to_string(),
            },
            None => "unknown".to_string(),
        }
    };

    let category = crate::llm::classify_input(&action.context, &location).await?;

    match category {
        crate::llm::InputCategory::Action => {
            println!("classified as: action");
            player::agent_tick(state, action).await
        }
        crate::llm::InputCategory::Query => {
            println!("classified as: query");
            // TODO: build context and return information without mutating world
            Ok(())
        }
        crate::llm::InputCategory::Dialogue => {
            println!("classified as: dialogue");
            // TODO: targeted NPC conversation
            Ok(())
        }
        crate::llm::InputCategory::Movement => {
            println!("classified as: movement");
            handlers::handle_movement( state, action).await
        }
    }
}