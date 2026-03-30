use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use crate::graph::{ESGraph, ESEdge};
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
    let equipped_ns  = format!("equipped/{}", player_name);
    let abilities_ns = format!("abilities/{}", player_name);
    let quests_ns    = format!("quests/{}", player_name);

    let allowed = vec![
        "world",
        inventory_ns.as_str(),
        equipped_ns.as_str(),
        abilities_ns.as_str(),
        quests_ns.as_str(),
    ];

    println!("calling ollama...");
    let patch_text = call_ollama(&context, &player_name).await?;
    println!("ollama responded:\n{}", patch_text);

    let patch = parse(&patch_text);
    println!("patch parsed, {} nodes", patch.nodes.len());

    // ── write block ────────────────────────────────────────────────
    {
        let mut graph = state.graph.write().await;
        merge_patch(&mut graph, patch, &allowed);
        println!("patch merged");

        // fix orphaned items — ensure they're connected to inventory container
        let inventory_key    = format!("inventory/{}/inventory:items", player_name);
        let inventory_prefix = format!("inventory/{}/item:", player_name);
        let quests_key       = format!("quests/{}/quests:active", player_name);
        let quests_prefix    = format!("quests/{}/quest:", player_name);

        let orphaned_items: Vec<String> = graph.nodes.keys()
            .filter(|k| k.starts_with(&inventory_prefix))
            .filter(|k| {
                graph.nodes.get(&inventory_key)
                    .map(|inv| !inv.edges.iter().any(|e| {
                        e.label == "contains" &&
                        format!("inventory/{}/{}:{}", player_name, e.target_type, e.target_id) == **k
                    }))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        for key in orphaned_items {
            let parts: Vec<&str> = key
                .split('/')
                .last()
                .unwrap_or("")
                .splitn(2, ':')
                .collect();
            if parts.len() == 2 {
                if let Some(inventory) = graph.nodes.get_mut(&inventory_key) {
                    inventory.edges.push(ESEdge::new("contains", parts[0], parts[1]));
                    println!("fixed orphaned item: {}", key);
                }
            }
        }

        let orphaned_quests: Vec<String> = graph.nodes.keys()
            .filter(|k| k.starts_with(&quests_prefix))
            .filter(|k| {
                graph.nodes.get(&quests_key)
                    .map(|q| !q.edges.iter().any(|e| {
                        e.label == "contains" &&
                        format!("quests/{}/{}:{}", player_name, e.target_type, e.target_id) == **k
                    }))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        for key in orphaned_quests {
            let parts: Vec<&str> = key
                .split('/')
                .last()
                .unwrap_or("")
                .splitn(2, ':')
                .collect();
            if parts.len() == 2 {
                if let Some(quests) = graph.nodes.get_mut(&quests_key) {
                    quests.edges.push(ESEdge::new("contains", parts[0], parts[1]));
                    println!("fixed orphaned quest: {}", key);
                }
            }
        }
    } // ── write lock released ────────────────────────────────────────

    // auto-generate stat blocks for new NPCs
    let new_npcs: Vec<(String, crate::graph::ESNode)> = {
        let graph = state.graph.read().await;
        graph.nodes.iter()
            .filter(|(k, _)| k.starts_with("npc:"))
            .filter(|(k, _)| {
                let npc_id = k.split(':').nth(1).unwrap_or("");
                !crate::stats::has_stat_block(&graph, npc_id)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    if !new_npcs.is_empty() {
        let mut graph = state.graph.write().await;
        for (key, node) in new_npcs {
            let npc_id = key.split(':').nth(1).unwrap_or("").to_string();
            let stats = crate::stats::generate_stats(&node);
            crate::stats::write_stat_block(&mut graph, &npc_id, &stats);
            println!("generated stat block for {}", npc_id);
        }
    }

    // persist
    {
        let graph = state.graph.read().await;
        if let Some(db) = &state.db {
            let db = db.lock().unwrap();
            if let Err(e) = crate::db::save_graph(&*db, &graph) {
                eprintln!("failed to persist world: {}", e);
            }
        }
    }

    // propagate signal
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

fn format_value(v: &crate::graph::ESValue) -> String {
    match v {
        crate::graph::ESValue::Text(s)   => s.clone(),
        crate::graph::ESValue::Number(n) => n.to_string(),
        crate::graph::ESValue::Bool(b)   => b.to_string(),
    }
}

fn build_context(graph: &ESGraph, player_id: &str, action: &str) -> String {
    let player = match graph.nodes.get(player_id) {
        Some(n) => n,
        None => return format!("Player {} not found", player_id),
    };

    let mut ctx = String::new();
    let player_name = player_id.split(':').nth(1).unwrap_or(player_id);

    // player state
    ctx.push_str("PLAYER STATE\n");
    ctx.push_str(&format!("id: {}\n", player_id));
    for (k, v) in &player.props {
        ctx.push_str(&format!("  {}: {}\n", k, format_value(v)));
    }

    // inventory — follow container
    let inventory_key = format!("inventory/{}/inventory:items", player_name);
    if let Some(inv) = graph.nodes.get(&inventory_key) {
        let items: Vec<_> = inv.edges.iter()
            .filter(|e| e.label == "contains")
            .filter_map(|e| {
                let k = format!("inventory/{}/{}:{}", player_name, e.target_type, e.target_id);
                graph.nodes.get(&k).map(|n| (k, n))
            })
            .collect();

        if !items.is_empty() {
            ctx.push_str("\nINVENTORY\n");
            for (key, node) in &items {
                ctx.push_str(&format!("  {}\n", key));
                for (k, v) in &node.props {
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
            }
        }
    }

    // equipped — follow container
    let equipped_key = format!("equipped/{}/equipped:slots", player_name);
    if let Some(equipped) = graph.nodes.get(&equipped_key) {
        let slots: Vec<_> = equipped.edges.iter()
            .filter_map(|e| {
                let k = format!("inventory/{}/{}:{}", player_name, e.target_type, e.target_id);
                graph.nodes.get(&k).map(|n| (e.label.clone(), k, n))
            })
            .collect();

        if !slots.is_empty() {
            ctx.push_str("\nEQUIPPED\n");
            for (slot, key, node) in &slots {
                ctx.push_str(&format!("  {} [{}]\n", key, slot));
                for (k, v) in &node.props {
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
            }
        }
    }

    // abilities — follow container
    let abilities_key = format!("abilities/{}/abilities:known", player_name);
    if let Some(ab) = graph.nodes.get(&abilities_key) {
        let abilities: Vec<_> = ab.edges.iter()
            .filter(|e| e.label == "contains")
            .filter_map(|e| {
                let k = format!("abilities/{}/{}:{}", player_name, e.target_type, e.target_id);
                graph.nodes.get(&k).map(|n| (k, n))
            })
            .collect();

        if !abilities.is_empty() {
            ctx.push_str("\nABILITIES\n");
            for (key, node) in &abilities {
                ctx.push_str(&format!("  {}\n", key));
                for (k, v) in &node.props {
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
            }
        }
    }

    // quests — follow container
    let quests_key = format!("quests/{}/quests:active", player_name);
    if let Some(q) = graph.nodes.get(&quests_key) {
        let quests: Vec<_> = q.edges.iter()
            .filter(|e| e.label == "contains")
            .filter_map(|e| {
                let k = format!("quests/{}/{}:{}", player_name, e.target_type, e.target_id);
                graph.nodes.get(&k).map(|n| (k, n))
            })
            .collect();

        if !quests.is_empty() {
            ctx.push_str("\nQUESTS\n");
            for (key, node) in &quests {
                ctx.push_str(&format!("  {}\n", key));
                for (k, v) in &node.props {
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
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
        if !ESGraph::is_world_key(id) { continue; }
        if let Some(crate::graph::ESValue::Text(loc)) = node.props.get("location") {
            if loc == &player_location {
                ctx.push_str(&format!("  {}\n", id));
                for (k, v) in &node.props {
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
            }
        }
    }

    ctx.push_str(&format!(
        "\nPLAYER REFERENCE\nAdd items to inventory: @inventory/{}/inventory:items --[contains]--> @inventory/{}/item:id\n",
        player_name, player_name
    ));
    ctx.push_str(&format!("\nCURRENT ACTION\n{}\n", action));

    ctx
}

fn build_namespace_docs(player_name: &str) -> String {
    let mut docs = String::new();
    docs.push_str("Namespaces and containers:\n");
    docs.push_str(&format!(
        "- Add item to inventory:\n  @inventory/{}/inventory:items\n    --[contains]--> @inventory/{}/item:unique_id\n  @inventory/{}/item:unique_id\n    name: \"Item Name\"\n    ...\n",
        player_name, player_name, player_name
    ));
    docs.push_str(&format!(
        "- Equip item (move from inventory to equipped):\n  @equipped/{}/equipped:slots\n    --[main_hand]--> @inventory/{}/item:id\n",
        player_name, player_name
    ));
    docs.push_str(&format!(
        "- Add ability:\n  @abilities/{}/abilities:known\n    --[contains]--> @abilities/{}/ability:id\n  @abilities/{}/ability:id\n    name: \"Ability Name\"\n    level: 1\n",
        player_name, player_name, player_name
    ));
    docs.push_str(&format!(
        "- Add quest:\n  @quests/{}/quests:active\n    --[contains]--> @quests/{}/quest:id\n  @quests/{}/quest:id\n    description: \"Quest description\"\n    status: active\n",
        player_name, player_name, player_name
    ));
    docs.push_str("\nNEVER write to stats/* — stats are system managed\n");
    docs.push_str("NEVER write to other players namespaces\n");
    docs.push_str("NEVER use owned_by or assigned_to edges — use container edges instead\n");
    docs
}

async fn call_ollama(context: &str, player_name: &str) -> Result<String, String> {
    let client = Client::new();
    let namespace_docs = build_namespace_docs(player_name);

    let prompt = format!(
        r#"You are an AI game master for a graph-based RPG.
Respond with ONLY valid Edgescript. No explanation, no markdown, no code blocks.

Edgescript rules:
- Every edge MUST be directly under its node declaration, indented 2 spaces
- NEVER write edges without a node declaration above them
- NEVER put edges inside property values
- Use container edges — NEVER use owned_by or assigned_to

{}

Always update the player node:
  narrative: updated description of who this player is
  dominant_trait: single word
  notable_actions: comma separated list

Output ONLY Edgescript. Nothing else.

Context:
{}

Edgescript patch:"#,
        namespace_docs,
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