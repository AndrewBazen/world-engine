use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use crate::graph::ESGraph;
use crate::parser::parse;

const OLLAMA_URL: &str = "http://192.168.1.120:11434/api/generate";
const MODEL: &str = "llama3.1:8b";

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub struct PlayerAction {
    pub player_id: String,
    pub context: String,
    pub strength: f64,
}

pub async fn agent_tick(
    state: Arc<AppState>,
    action: PlayerAction,
) -> Result<(), String> {
    println!("agent tick fired for player: {}", action.player_id);
    
    let context = {
        let graph = state.graph.read().await;
        let ctx = build_context(&graph, &action.player_id, &action.context);
        println!("context built:\n{}", ctx);
        ctx
    };

    println!("calling ollama...");
    let patch_text = call_ollama(&context).await?;
    println!("ollama responded:\n{}", patch_text);

    let patch = parse(&patch_text);
    println!("patch parsed, {} nodes", patch.nodes.len());

    {
        let mut graph = state.graph.write().await;
        merge_patch(&mut graph, patch);
        println!("patch merged");
    }

    let signal = crate::signal::EventSignal::new(
        &action.player_id,
        action.strength,
        &action.context,
    );
    crate::signal::propagate(state.clone(), signal).await;
    println!("signal propagated");

    let snapshot = crate::server::build_snapshot(&state).await;
    let _ = state.tx.send(snapshot);

    Ok(())
}

fn build_context(graph: &ESGraph, player_id: &str, action: &str) -> String {
    let player = match graph.nodes.get(player_id) {
        Some(n) => n,
        None => return format!("Player {} not found", player_id),
    };

    let mut ctx = String::new();

    ctx.push_str("PLAYER STATE\n");
    ctx.push_str(&format!("id: {}\n", player_id));
    for (k, v) in &player.props {
        let display = match v {
            crate::graph::ESValue::Text(s)   => s.clone(),
            crate::graph::ESValue::Number(n) => n.to_string(),
            crate::graph::ESValue::Bool(b)   => b.to_string(),
        };
        ctx.push_str(&format!("  {}: {}\n", k, display));
    }

    // current location
    let player_location = match player.props.get("location") {
        Some(crate::graph::ESValue::Text(l)) => l.clone(),
        _ => String::from("unknown"),
    };

    ctx.push_str(&format!("\nCURRENT LOCATION: {}\n", player_location));
    ctx.push_str("\nNEARBY\n");

    // find all nodes in the same location
    for (id, node) in &graph.nodes {
        if id == player_id { continue; }
        if let Some(crate::graph::ESValue::Text(loc)) = node.props.get("location") {
            if loc == &player_location {
                ctx.push_str(&format!("  {}\n", id));
                for (k, v) in &node.props {
                    let display = match v {
                        crate::graph::ESValue::Text(s)   => s.clone(),
                        crate::graph::ESValue::Number(n) => n.to_string(),
                        crate::graph::ESValue::Bool(b)   => b.to_string(),
                    };
                    ctx.push_str(&format!("    {}: {}\n", k, display));
                }
            }
        }
    }

    ctx.push_str(&format!(
        "\nPLAYER REFERENCE\nTo connect items to this player use: --[owned_by]--> @{}\n", 
        player_id
    ));
    ctx.push_str(&format!("\nCURRENT ACTION\n{}\n", action));
    ctx
}

async fn call_ollama(context: &str) -> Result<String, String> {
    let client = Client::new();

    let prompt = format!(
        r#"You are an AI game master for a graph-based RPG.
    Respond with ONLY valid Edgescript. No explanation, no markdown, no code blocks.
    
    Edgescript format rules:
    - Node declaration: @type:id
    - Property with 2 space indent:  key: value
    - String values use quotes: name: "The Sword"
    - Number values no quotes: damage: 8
    - Boolean values: alive: true
    - Edge with 2 space indent:  --[label]--> @type:id
    - Always connect newly obtained items to the player with --[owned_by]--> @player:id
    - Do not invent system nodes like equipment slots unless explicitly needed
    - Always update the player node with:
        narrative: updated 1-2 sentence description of who this player is based on their history
        dominant_trait: single word describing their playstyle  
        notable_actions: comma separated list of significant actions so far
    
    Example of valid Edgescript:
    @item:shadow_blade
      name: "Shadow Blade"
      damage: 12
      rarity: rare
      --[owned_by]--> @player:andrew
    
    @player:andrew
      courage: 15
    
    Output ONLY Edgescript like the example above. Nothing else.
    
    Context:
    {}
    
    Edgescript patch:"#,
        context
    );

    let req = OllamaRequest {
        model: MODEL.to_string(),
        prompt,
        stream: false,
    };

    let res = client
        .post(OLLAMA_URL)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("ollama request failed: {}", e))?;

    let body: OllamaResponse = res
        .json()
        .await
        .map_err(|e| format!("failed to parse ollama response: {}", e))?;

    Ok(body.response)
}

fn merge_patch(world: &mut ESGraph, patch: ESGraph) {
    for (key, patch_node) in patch.nodes {
        if let Some(existing) = world.nodes.get_mut(&key) {
            // merge props
            for (k, v) in patch_node.props {
                existing.props.insert(k, v);
            }
            // merge edges — add any new ones
            for edge in patch_node.edges {
                let already_exists = existing.edges.iter().any(|e| {
                    e.label == edge.label
                    && e.target_type == edge.target_type
                    && e.target_id == edge.target_id
                });
                if !already_exists {
                    existing.edges.push(edge);
                }
            }
        } else {
            // new node — insert directly
            world.nodes.insert(key, patch_node);
        }
    }
}