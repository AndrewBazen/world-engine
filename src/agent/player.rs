use super::{PlayerAction, format_value, merge_patch, VERBOSE};
use crate::agent::npc::npc_agent_tick;
use crate::graph::{ESGraph, ESEdge, parse};
use std::collections::HashSet;
use std::sync::Arc;
use crate::state::AppState;

pub async fn agent_tick(
    state: Arc<AppState>,
    action: PlayerAction,
) -> Result<(), String> {
    println!("agent tick fired for player: {}", action.player_id);

    let context = {
        let graph = state.graph.read().await;
        let ctx = build_context(&graph, &action.player_id, &action.context);
        if VERBOSE {
            println!("context built:\n{}", ctx);
        }
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
    let patch_text = call_player_agent(&context, &player_name).await?;
    if VERBOSE {
        println!("ollama responded:\n{}", patch_text);
    }

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
    let (absorbed, visited): (Vec<_>, HashSet<_>) = crate::signal::propagate(state.clone(), signal).await;
    println!("signal propagated, {} NPCs absorbed", absorbed.len());

    // fire NPC agent ticks for each absorbed NPC — collect any emitted signals
    let mut npc_signals: Vec<crate::signal::EventSignal> = Vec::new();
    for npc_signal in &absorbed {
        // extract NPC props for the classifier
        let (occupation, personality, relationships) = {
            let graph = state.graph.read().await;
            match graph.nodes.get(&npc_signal.npc_id) {
                Some(n) => {
                    let occ = match n.props.get("occupation") {
                        Some(crate::graph::ESValue::Text(s)) => s.clone(),
                        _ => "unknown".to_string(),
                    };
                    let pers = match n.props.get("personality") {
                        Some(crate::graph::ESValue::Text(s)) => s.clone(),
                        _ => "unknown".to_string(),
                    };
                    let rels = n.edges.iter()
                        .map(|e| format!("{} {}:{}", e.label, e.target_type, e.target_id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    (occ, pers, rels)
                }
                None => continue,
            }
        };
    
        let npc_name = npc_signal.npc_id.split(':').nth(1).unwrap_or(&npc_signal.npc_id);
    
        let should_act = crate::llm::should_npc_act(
            npc_name,
            &occupation,
            &personality,
            &relationships,
            &npc_signal.context,
            npc_signal.strength,
        ).await.unwrap_or(false);
    
        if should_act {
            println!("  {} decides to act", npc_signal.npc_id);
            match npc_agent_tick(state.clone(), npc_signal).await {
                Ok(Some(emitted)) => {
                    println!("  {} emitted signal: {}", npc_signal.npc_id, emitted.context);
                    npc_signals.push(emitted);
                }
                Ok(None) => {
                    println!("  {} reacted quietly", npc_signal.npc_id);
                }
                Err(e) => {
                    eprintln!("  npc agent error for {}: {}", npc_signal.npc_id, e);
                }
            }
        } else {
            println!("  {} ignores the event", npc_signal.npc_id);
            // still write a memory
            let location = {
                let graph = state.graph.read().await;
                graph.nodes.get(&npc_signal.npc_id)
                    .and_then(|n| match n.props.get("location") {
                        Some(crate::graph::ESValue::Text(l)) => Some(l.clone()),
                        _ => None,
                    })
                    .unwrap_or_default()
            };
            let significance = crate::memory::calculate_significance(
                npc_signal.strength,
                npc_signal.context.contains(npc_name),
            );
            let event = crate::memory::MemoryEvent::new(
                npc_signal.origin_id.clone(),
                npc_signal.context.clone(),
                String::new(),
                "ignored".to_string(),
                location,
                significance,
            );
            let mut graph = state.graph.write().await;
            crate::memory::write_memory(&mut graph, &npc_signal.npc_id, &event);
            println!("  wrote passive memory for {}", npc_signal.npc_id);
        }
    }

    // propagate any NPC-emitted signals (cascading reactions)
    for npc_signal in npc_signals {
        let cascade = crate::signal::EventSignal::with_visited(
            &npc_signal.origin_id, 
            npc_signal.strength, 
            &npc_signal.context, 
            visited.clone(),
        );
        let (_cascade_absorbed, _): (Vec<_>, HashSet<_>) = crate::signal::propagate(state.clone(), cascade).await;
        println!("cascade signal propagated, {} NPCs absorbed", _cascade_absorbed.len());
        // NOTE: we don't recurse NPC agent ticks on cascades to prevent infinite loops.
        // A future version could allow bounded depth.
    }

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


async fn call_player_agent(context: &str, player_name: &str) -> Result<String, String> {
    let namespace_docs = build_namespace_docs(player_name);

    let prompt = format!(
        r#"You are an AI game master for a graph-based RPG.
    Respond with ONLY valid Edgescript. No explanation, no markdown, no code blocks.

    EDGESCRIPT SYNTAX
    @type:id                  — world node declaration
    @namespace/type:id        — namespaced node declaration
    key: value                — property (indented under its node)
    --[label]--> @type:id     — edge (indented under its node)

    Every node MUST have a colon between type and id. The colon is required.
    CORRECT: @player:andrew
    CORRECT: @npc:guard
    CORRECT: @inventory/{player_name}/item:sword
    WRONG:   @player/andrew
    WRONG:   @inventory/{player_name}/item/sword

    EXAMPLE PATCH
    @player:{player_name}
    narrative: "Stole a loaf of bread from the baker's stall."
    dominant_trait: reckless
    notable_actions: stole bread

    @inventory/{player_name}/inventory:items
    --[contains]--> @inventory/{player_name}/item:bread

    @inventory/{player_name}/item:bread
    name: "Stolen Bread"
    weight: 1

    @npc:baker
    disposition: hostile
    narrative: "Noticed the theft and is furious."

    RULES
    - Every edge MUST be directly under its node declaration, indented 2 spaces
    - NEVER write edges without a node declaration above them
    - NEVER put edges inside property values
    - Use container edges — NEVER use owned_by or assigned_to
    - NEVER write to stats/* — stats are system managed
    - NEVER write to other players' namespaces

    {namespace_docs}

    Always update the player node @player:{player_name} with:
    narrative: updated description of who this player is
    dominant_trait: single word
    notable_actions: comma separated list

    Output ONLY Edgescript. Nothing else.

    Context:
    {context}

    Edgescript patch:"#,
        player_name = player_name,
        namespace_docs = namespace_docs,
        context = context,
    );
    crate::llm::call_ollama(crate::llm::PLAYER_MODEL, &prompt).await
}


