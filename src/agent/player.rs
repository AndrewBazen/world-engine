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

        // New people are allowed — the world is supposed to grow. They just
        // have to be well-formed and distinguishable from the cast that
        // already exists.
        let mut patch = patch;
        patch.nodes.retain(|key, node| {
            let is_character = key.starts_with("npc:") || key.starts_with("player:");
            if !is_character || graph.nodes.contains_key(key) {
                return true;
            }
            match validate_new_character(key, node, &graph) {
                Ok(()) => {
                    println!("new character enters the world: {}", key);
                    true
                }
                Err(why) => {
                    println!("rejected new character {}: {}", key, why);
                    false
                }
            }
        });

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

    // What onlookers actually perceive. The raw input is first person — "i draw
    // my sword" — and every NPC that absorbed it read that as their OWN action,
    // which is why a guard three districts away reported spotting a theft.
    // The agent supplies a third-person description instead.
    let observed = {
        let mut graph = state.graph.write().await;
        graph
            .nodes
            .get_mut(&action.player_id)
            .and_then(|p| {
                let text = match p.props.get("signal_emit") {
                    Some(crate::graph::ESValue::Text(s)) if !s.trim().is_empty() => s.clone(),
                    _ => return None,
                };
                // an instruction, not state
                p.props.remove("signal_emit");
                Some(text)
            })
            .unwrap_or_else(|| format!("{}: {}", player_name, action.context))
    };
    println!("observed as: {}", observed);

    // propagate signal
    let signal = crate::signal::EventSignal::new(
        &action.player_id,
        action.strength,
        &observed,
    );
    let (absorbed, _visited): (Vec<_>, HashSet<_>) = crate::signal::propagate(state.clone(), signal).await;
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

    // ── Round two: reactions to the reactions ──────────────────────
    //
    // These start fresh rather than inheriting the first signal's visited set.
    // Carrying it forward meant everyone who heard the original event was
    // excluded, so a guard shouting "Stop, thief!" reached precisely nobody.
    //
    // Bounded by construction: only NPCs that have not already acted this turn
    // get a tick, and there is no round three. In a small scene where everyone
    // already reacted this costs zero extra model calls — the cascade is then
    // just awareness and animation.
    let mut already_acted: HashSet<String> =
        absorbed.iter().map(|a| a.npc_id.clone()).collect();
    let mut second_round: Vec<crate::signal::AbsorbedSignal> = Vec::new();

    // Several NPCs reacting to one event routinely emit the same line — three
    // people shouting "Stop, thief!" is one event in the world, not three.
    // Propagate each distinct utterance once, at its loudest.
    let npc_signals = dedupe_by_context(npc_signals);

    for npc_signal in npc_signals {
        let cascade = crate::signal::EventSignal::new(
            &npc_signal.origin_id,
            npc_signal.strength,
            &npc_signal.context,
        );
        let (cascade_absorbed, _): (Vec<_>, HashSet<_>) =
            crate::signal::propagate(state.clone(), cascade).await;

        let fresh: Vec<_> = cascade_absorbed
            .into_iter()
            .filter(|a| !already_acted.contains(&a.npc_id))
            .collect();

        println!(
            "cascade propagated, {} newly involved",
            fresh.len()
        );

        for a in fresh {
            already_acted.insert(a.npc_id.clone());
            second_round.push(a);
        }
    }

    for npc_signal in &second_round {
        println!("  {} reacts to the commotion", npc_signal.npc_id);
        match npc_agent_tick(state.clone(), npc_signal).await {
            Ok(_) => {}
            Err(e) => eprintln!("  npc agent error for {}: {}", npc_signal.npc_id, e),
        }
    }

    let snapshot = crate::server::build_snapshot(&state).await;
    let _ = state.tx.send(snapshot);

    Ok(())
}




/// Collapse emitted signals that say the same thing, keeping the loudest.
fn dedupe_by_context(
    signals: Vec<crate::signal::EventSignal>,
) -> Vec<crate::signal::EventSignal> {
    let mut best: Vec<crate::signal::EventSignal> = Vec::new();

    for signal in signals {
        let key = signal.context.trim().to_lowercase();
        match best
            .iter_mut()
            .find(|s| s.context.trim().to_lowercase() == key)
        {
            Some(existing) => {
                if signal.strength > existing.strength {
                    *existing = signal;
                }
            }
            None => best.push(signal),
        }
    }

    best
}

#[cfg(test)]
mod cascade_tests {
    use super::*;
    use crate::signal::EventSignal;

    #[test]
    fn test_identical_shouts_collapse_to_the_loudest() {
        let signals = vec![
            EventSignal::new("npc:thomas_pellar", 0.8, "shouts: Stop, thief!"),
            EventSignal::new("npc:john_smith", 0.9, "shouts: Stop, thief!"),
            EventSignal::new("npc:jin_lyons", 0.5, "slips into the crowd"),
        ];

        let out = dedupe_by_context(signals);

        assert_eq!(out.len(), 2, "one shout and one slip, not three propagations");
        let shout = out
            .iter()
            .find(|s| s.context.contains("thief"))
            .expect("the shout survives");
        assert_eq!(shout.origin_id, "npc:john_smith", "the loudest one wins");
        assert_eq!(shout.strength, 0.9);
    }
}

/// Whether the world may accept this as a new person.
///
/// Emergence needs new characters to be able to appear, so this is deliberately
/// permissive about *who* — it only rejects the four ways a model produces
/// something that isn't a person at all: a malformed id, a type word used as a
/// name, a duplicate of someone already here under a different node type, and
/// an empty shell with no description.
fn validate_new_character(
    key: &str,
    node: &crate::graph::ESNode,
    world: &ESGraph,
) -> Result<(), String> {
    let name = node.id.trim();

    if name.is_empty() || name.contains(':') || name.contains('/') || name.contains(' ') {
        return Err(format!("malformed id `{}`", node.id));
    }

    // Models reach for the type word as a name constantly — @npc:npc, and
    // @npc:player:andrew, which parses out to an id of `player:andrew`.
    if matches!(name, "player" | "npc" | "id" | "name" | "unknown" | "someone") {
        return Err(format!("`{}` is a type word, not a name", name));
    }

    // The same person cannot exist as both a player and an NPC.
    for other_type in ["player", "npc"] {
        let collision = format!("{}:{}", other_type, name);
        if collision != key && world.nodes.contains_key(&collision) {
            return Err(format!("already in the world as {}", collision));
        }
    }

    // A character with nothing said about it is noise, not a person — and it
    // would get a default stat block and start absorbing signals.
    const DESCRIPTIVE: [&str; 7] = [
        "name", "occupation", "personality", "narrative", "build", "background", "description",
    ];
    if !DESCRIPTIVE.iter().any(|k| node.props.contains_key(*k)) {
        return Err("no descriptive properties".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod character_tests {
    use super::*;
    use crate::graph::{ESNode, ESValue};

    fn world_with_andrew() -> ESGraph {
        let mut g = ESGraph::new();
        g.insert(ESNode::new("world", "player", "andrew"));
        g
    }

    #[test]
    fn test_a_described_stranger_is_allowed() {
        let world = world_with_andrew();
        let node = ESNode::new("world", "npc", "mira_fenn")
            .with_prop("occupation", ESValue::Text("innkeeper".to_string()));
        assert!(validate_new_character("npc:mira_fenn", &node, &world).is_ok());
    }

    #[test]
    fn test_type_words_are_rejected() {
        let world = world_with_andrew();
        for bad in ["player", "npc", "unknown"] {
            let node = ESNode::new("world", "npc", bad)
                .with_prop("occupation", ESValue::Text("guard".to_string()));
            assert!(
                validate_new_character(&format!("npc:{}", bad), &node, &world).is_err(),
                "`{}` should not be accepted as a name",
                bad
            );
        }
    }

    #[test]
    fn test_duplicate_of_the_player_is_rejected() {
        let world = world_with_andrew();
        let node = ESNode::new("world", "npc", "andrew")
            .with_prop("occupation", ESValue::Text("adventurer".to_string()));
        assert!(validate_new_character("npc:andrew", &node, &world).is_err());
    }

    #[test]
    fn test_malformed_id_is_rejected() {
        let world = world_with_andrew();
        let node = ESNode::new("world", "npc", "player:andrew")
            .with_prop("name", ESValue::Text("Andrew".to_string()));
        assert!(validate_new_character("npc:player:andrew", &node, &world).is_err());
    }

    #[test]
    fn test_empty_shell_is_rejected() {
        let world = world_with_andrew();
        let node = ESNode::new("world", "npc", "guard");
        assert!(
            validate_new_character("npc:guard", &node, &world).is_err(),
            "a character with nothing said about it is not a person"
        );
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use crate::graph::{ESNode, ESValue};

    #[test]
    fn test_empty_inventory_is_stated_explicitly() {
        let mut graph = ESGraph::new();
        graph.insert(
            ESNode::new("world", "player", "andrew")
                .with_prop("location", ESValue::Text("market".to_string())),
        );

        let ctx = build_context(&graph, "player:andrew", "draw my sword");

        assert!(ctx.contains("INVENTORY"), "INVENTORY section must always appear");
        assert!(
            ctx.contains("empty"),
            "the model must be told the inventory is empty, or it invents items:\n{}",
            ctx
        );
    }

    #[test]
    fn test_items_appear_without_a_container_node() {
        let mut graph = ESGraph::new();
        graph.insert(ESNode::new("world", "player", "andrew"));
        graph.insert(
            ESNode::new("inventory/andrew", "item", "shadow_blade")
                .with_prop("name", ESValue::Text("Shadow Blade".to_string())),
        );

        let ctx = build_context(&graph, "player:andrew", "draw my blade");

        assert!(
            ctx.contains("Shadow Blade"),
            "items must reach the prompt even with no container node:\n{}",
            ctx
        );
    }

    #[test]
    fn test_context_is_deterministic() {
        let mut graph = ESGraph::new();
        graph.insert(
            ESNode::new("world", "player", "andrew")
                .with_prop("location", ESValue::Text("market".to_string()))
                .with_prop("name", ESValue::Text("Andrew".to_string()))
                .with_prop("courage", ESValue::Number(14.0)),
        );

        let a = build_context(&graph, "player:andrew", "wait");
        let b = build_context(&graph, "player:andrew", "wait");
        assert_eq!(a, b, "the same world must produce the same prompt");
    }
}

/// Write a node's properties in a stable order. `props` is a HashMap, so
/// unsorted iteration made an unchanged world produce a different prompt on
/// every run — and therefore a different completion.
fn push_props(ctx: &mut String, node: &crate::graph::ESNode, indent: &str) {
    let mut props: Vec<_> = node.props.iter().collect();
    props.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in props {
        ctx.push_str(&format!("{}{}: {}\n", indent, k, format_value(v)));
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
    push_props(&mut ctx, player, "  ");

    // inventory — scan the namespace directly rather than following container
    // edges. A missing or unlinked container used to hide this section
    // entirely, and the model would then invent whatever the action needed.
    // This section is ALWAYS emitted, including when empty.
    let inventory_prefix = format!("inventory/{}/item:", player_name);
    let mut items: Vec<_> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&inventory_prefix))
        .collect();
    items.sort_by(|a, b| a.0.cmp(b.0));

    ctx.push_str("\nINVENTORY\n");
    if items.is_empty() {
        ctx.push_str("  (empty — this player is carrying nothing)\n");
    } else {
        for (key, node) in &items {
            ctx.push_str(&format!("  {}\n", key));
            push_props(&mut ctx, node, "    ");
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

    // abilities — same treatment as inventory, and always emitted
    let abilities_prefix = format!("abilities/{}/ability:", player_name);
    let mut abilities: Vec<_> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&abilities_prefix))
        .collect();
    abilities.sort_by(|a, b| a.0.cmp(b.0));

    ctx.push_str("\nABILITIES\n");
    if abilities.is_empty() {
        ctx.push_str("  (none — this player has learned no abilities)\n");
    } else {
        for (key, node) in &abilities {
            ctx.push_str(&format!("  {}\n", key));
            push_props(&mut ctx, node, "    ");
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

    let mut nearby: Vec<_> = graph.nodes.iter()
        .filter(|(id, _)| *id != player_id)
        .filter(|(id, _)| ESGraph::is_world_key(id))
        .filter(|(_, node)| {
            matches!(node.props.get("location"),
                Some(crate::graph::ESValue::Text(loc)) if loc == &player_location)
        })
        .collect();
    nearby.sort_by(|a, b| a.0.cmp(b.0));

    if nearby.is_empty() {
        ctx.push_str("  (nobody else is here)\n");
    } else {
        for (id, node) in &nearby {
            ctx.push_str(&format!("  {}\n", id));
            push_props(&mut ctx, node, "    ");
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

    GROUNDING — THE MOST IMPORTANT RULE
    The CONTEXT below is the complete and only truth about this world.
    If the player's action needs an item, ability, quest, person or place that
    does NOT appear in the CONTEXT, the action FAILS.
    When an action fails, do NOT create the missing thing. Write only a
    narrative describing the failure, and change nothing else.

    Match what the player names to what they HAVE, by kind rather than by exact
    wording. A "Shadow Blade" is a blade, a sword, and a weapon. A "Brass
    Lantern" is a lantern and a light. If anything in the CONTEXT reasonably
    answers to what the player said, the action SUCCEEDS — do not refuse it on a
    word mismatch.

    Only refuse when nothing in the CONTEXT can answer at all. Example — the
    player says "read the letter" and INVENTORY holds no letter or paper:
    @player:{player_name}
    narrative: "Searched every pocket for a letter that was never there."
    dominant_trait: confused
    notable_actions: looked for a letter they do not have

    Never invent an item to make an action succeed.
    You may only add an item when the CONTEXT shows where it came from —
    something present that was taken, bought, found, or given.

    NEW PEOPLE
    The world is allowed to grow. A new person MAY appear when the action would
    plausibly bring one — entering a building, drawing a crowd, someone arriving.
    When you add one:
    - give them a real personal name, never a role word
      CORRECT: @npc:mira_fenn, @npc:oda_veyle
      WRONG:   @npc:guard, @npc:merchant, @npc:npc, @npc:player
    - give them at least occupation and personality
    - put them at the player's location
    Never add someone who is already in the world under a different name.

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

    Edge targets MUST also be written as @type:id.
    CORRECT: --[located_in]--> @location:market_district
    WRONG:   --[located_in]--> market_district

    FORMAT EXAMPLE — this shows shape only.
    The contents are placeholders. Never copy these names into your answer.
    @player:{player_name}
    narrative: "One sentence describing what just happened to this player."
    dominant_trait: single_word
    notable_actions: short, comma, separated

    @inventory/{player_name}/inventory:items
    --[contains]--> @inventory/{player_name}/item:placeholder_id

    @inventory/{player_name}/item:placeholder_id
    name: "Item Name"
    weight: 1

    RULES
    - Declare each node ONCE. A second declaration of the same node discards the first.
    - Every edge MUST be directly under its node declaration
    - NEVER write edges without a node declaration above them
    - NEVER put edges inside property values
    - Use container edges — NEVER use owned_by or assigned_to
    - NEVER write to stats/* — stats are system managed
    - NEVER write to other players' namespaces

    {namespace_docs}

    Always update the player node @player:{player_name} with:
    narrative: what just happened, in one sentence
    dominant_trait: single word
    notable_actions: comma separated list
    signal_emit: what a BYSTANDER would see, third person, one short clause

    signal_emit is read by everyone nearby, so it must never be first person.
    CORRECT: signal_emit: "{player_name} draws a blade in the middle of the market"
    WRONG:   signal_emit: "i draw my sword"

    Output ONLY Edgescript. Nothing else.

    Context:
    {context}

    Edgescript patch:"#,
        player_name = player_name,
        namespace_docs = namespace_docs,
        context = context,
    );
    crate::llm::call_ollama(crate::llm::player_model(), &prompt).await
}


