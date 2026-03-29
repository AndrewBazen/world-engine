use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use crate::graph::{ESGraph,ESEdge};
use crate::parser::parse;

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const PLAYER_MODEL: &str = "llama3.1:8b-instruct-q8_0";

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

    let player_name = action.player_id
        .split(':')
        .nth(1)
        .unwrap_or(&action.player_id)
        .to_string();

    let inventory_ns = format!("inventory/{}", player_name);
    let abilities_ns = format!("abilities/{}", player_name);
    let quests_ns = format!("quests/{}", player_name);
    let allowed = vec![
        "world",
        inventory_ns.as_str(),
        abilities_ns.as_str(),
        quests_ns.as_str(),
    ];

    println!("calling ollama...");
    let patch_text = call_ollama(&context, &player_name).await?;
    println!("ollama responded:\n{}", patch_text);

    let patch = parse(&patch_text);
    println!("patch parsed, {} nodes", patch.nodes.len());

    // ── write block — lock acquired and released here ──────────────
    {
        let mut graph = state.graph.write().await;
        merge_patch(&mut graph, patch, &allowed);
        println!("patch merged");

        let inventory_prefix = format!("inventory/{}/", player_name);
        let quests_prefix = format!("quests/{}/", player_name);

        let orphaned_inventory: Vec<String> = graph.nodes.iter()
            .filter(|(k, v)| {
                k.starts_with(&inventory_prefix) &&
                !v.edges.iter().any(|e| e.label == "owned_by")
            })
            .map(|(k, _)| k.clone())
            .collect();

        let orphaned_quests: Vec<String> = graph.nodes.iter()
            .filter(|(k, v)| {
                k.starts_with(&quests_prefix) &&
                !v.edges.iter().any(|e| e.label == "assigned_to")
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in orphaned_inventory {
            if let Some(node) = graph.nodes.get_mut(&key) {
                node.edges.push(ESEdge::new("owned_by", "player", &player_name));
                println!("fixed orphaned inventory node: {}", key);
            }
        }

        for key in orphaned_quests {
            if let Some(node) = graph.nodes.get_mut(&key) {
                node.edges.push(ESEdge::new("assigned_to", "player", &player_name));
                println!("fixed orphaned quest node: {}", key);
            }
        }
    } // ── write lock released here ───────────────────────────────────

    // simpler version — save everything after agent tick
    {
        let graph = state.graph.read().await;
        if let Some(db) = &state.db {
            if let Err(e) = crate::db::save_graph(db, &graph).await {
                eprintln!("failed to persist world: {}", e);
            }
        }
    }

    // now propagate and snapshot can acquire the lock
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

    // world identity
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

    // inventory namespace
    let player_name = player_id.split(':').nth(1).unwrap_or(player_id);
    let inventory_prefix = format!("inventory/{}/", player_name);
    let inventory_nodes: Vec<_> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&inventory_prefix))
        .collect();

    if !inventory_nodes.is_empty() {
        ctx.push_str("\nINVENTORY\n");
        for (key, node) in &inventory_nodes {
            ctx.push_str(&format!("  {}\n", key));
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

    // abilities namespace
    let abilities_prefix = format!("abilities/{}/", player_name);
    let ability_nodes: Vec<_> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&abilities_prefix))
        .collect();

    if !ability_nodes.is_empty() {
        ctx.push_str("\nABILITIES\n");
        for (key, node) in &ability_nodes {
            ctx.push_str(&format!("  {}\n", key));
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

    // current location and nearby world nodes
    let player_location = match player.props.get("location") {
        Some(crate::graph::ESValue::Text(l)) => l.clone(),
        _ => String::from("unknown"),
    };

    ctx.push_str(&format!("\nCURRENT LOCATION: {}\n", player_location));
    ctx.push_str("\nNEARBY\n");

    for (id, node) in &graph.nodes {
        if id == player_id { continue; }
        if !ESGraph::is_world_key(id) { continue; }  // world nodes only
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

fn build_namespace_docs(player_name: &str) -> String {
    let mut docs = String::new();
    docs.push_str("Namespaces you are allowed to write to:\n");
    docs.push_str("- World entities you directly affect: @type:id\n");
    docs.push_str(&format!("- Player inventory: @inventory/{}/item:id\n", player_name));
    docs.push_str(&format!("- Player abilities: @abilities/{}/ability:id\n", player_name));
    docs.push_str(&format!("- Player quests: @quests/{}/quest:id\n", player_name));
    docs.push_str("\nNamespaces you are NOT allowed to write to:\n");
    docs.push_str("- NPC memory: memory/* — NPCs manage their own memory\n");
    docs.push_str("- Other players: inventory/other_player/* \n");
    docs.push_str("- System nodes: remove/*, signal/*\n");
    docs
}

async fn call_ollama(context: &str, player_name: &str) -> Result<String, String> {
    let client = Client::new();

    let prompt = format!(
        r#"You are an AI game master for a graph-based RPG.
    Respond with ONLY valid Edgescript. No explanation, no markdown, no code blocks.
    
    Edgescript rules:
    - Every edge MUST be directly under its node declaration, indented 2 spaces
    - NEVER write edges without a node declaration above them
    - NEVER modify existing inventory items — create new ones for new loot
    - NEVER put edges inside property values
    - Every new inventory item MUST have --[owned_by]--> @player:{}
    - Every new quest MUST have --[assigned_to]--> @player:{}
    
    Example of correct Edgescript:
    @inventory/{}/item:stolen_pouch
      name: "Merchant's Pouch"
      value: 12
      rarity: common
      --[owned_by]--> @player:{}
    
    @quests/{}/quest:find_the_fence
      description: "Find someone to buy your stolen goods"
      status: active
      --[assigned_to]--> @player:{}
    
    @player:{}
      narrative: "updated narrative here"
      dominant_trait: cunning
      notable_actions: "pickpocketed merchant"
    
    @npc:merchant
      disposition: uneasy
    
    Output ONLY Edgescript like the example. Nothing else.
    
    {}
    
    Context:
    {}
    
    Edgescript patch:"#,
            player_name,  // owned_by rule
            player_name,  // assigned_to rule
            player_name,  // example inventory
            player_name,  // example quest
            player_name,  // example owned_by edge
            player_name,  // example assigned_to edge
            player_name,  // example player update
            build_namespace_docs(player_name),
            context
        );

    let req = OllamaRequest {
        model: PLAYER_MODEL.to_string(),
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

pub fn merge_patch(world: &mut ESGraph, patch: ESGraph, allowed_namespaces: &[&str]) {
    for (key, patch_node) in patch.nodes {
        if key.starts_with("remove:") { continue; }

        // check namespace is allowed
        let namespace = &patch_node.namespace;
        let is_allowed = allowed_namespaces.iter().any(|ns| {
            namespace == *ns || namespace.starts_with(ns)
        });

        if !is_allowed {
            println!("rejected write to namespace: {}", namespace);
            continue;
        }

        // rest of merge unchanged
        if let Some(existing) = world.nodes.get_mut(&key) {
            for (k, v) in patch_node.props {
                existing.props.insert(k, v);
            }
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
            world.nodes.insert(key, patch_node);
        }
    }
}