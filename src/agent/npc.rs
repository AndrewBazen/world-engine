use super::{format_value, merge_patch, VERBOSE};
use crate::graph::{ESGraph, parse};
use std::sync::Arc;


use crate::state::AppState;

pub async fn npc_agent_tick(
    state: Arc<AppState>,
    signal: &crate::signal::AbsorbedSignal,
) -> Result<Option<crate::signal::EventSignal>, String> {
    let npc_id = &signal.npc_id;
    println!("npc agent tick fired for: {}", npc_id);

    let context = {
        let graph = state.graph.read().await;
        build_npc_context(&graph, npc_id, &signal.context, signal.strength)
    };
    if VERBOSE {
        println!("npc context built:\n{}", context);
    }
    
    let npc_name = npc_id
        .split(':')
        .nth(1)
        .unwrap_or(npc_id)
        .to_string();

    // NPCs can only write to world namespace
    let allowed = vec!["world"];

    println!("calling ollama for npc {}...", npc_name);
    let patch_text = call_npc_agent(&context, &npc_name).await?;
    if VERBOSE {
        println!("ollama npc responded:\n{}", patch_text);
    }
    
    let patch = parse(&patch_text);
    println!("npc patch parsed, {} nodes", patch.nodes.len());

    // check if the NPC decided to emit a signal (look for signal_emit prop)
    let emitted_signal = patch.nodes.values()
        .find(|n| n.id == npc_name && n.node_type == "npc")
        .and_then(|n| {
            let context = match n.props.get("signal_emit") {
                Some(crate::graph::ESValue::Text(s)) => s.clone(),
                _ => return None,
            };
            let strength = match n.props.get("signal_strength") {
                Some(crate::graph::ESValue::Number(v)) => *v,
                _ => 0.5,
            };
            Some((context, strength))
        });

    // merge the patch
    {
        let mut graph = state.graph.write().await;
        merge_patch(&mut graph, patch, &allowed);
        println!("npc patch merged for {}", npc_name);

        // clean up signal_emit/signal_strength — these are instructions, not state
        if let Some(npc_node) = graph.nodes.get_mut(npc_id) {
            npc_node.props.remove("signal_emit");
            npc_node.props.remove("signal_strength");
        }
    }

  // write memory of this event
    {
        let location = {
            let graph = state.graph.read().await;
            graph.nodes.get(npc_id)
                .and_then(|n| match n.props.get("location") {
                    Some(crate::graph::ESValue::Text(l)) => Some(l.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        };

        let name = npc_id.split(':').nth(1).unwrap_or(npc_id);
        let is_direct = signal.context.contains(name);
        let significance = crate::memory::calculate_significance(signal.strength, is_direct);

        let event = crate::memory::MemoryEvent::new(
            signal.origin_id.clone(),
            signal.context.clone(),
            String::new(),
            String::new(),
            location,
            significance,
        );

        let mut graph = state.graph.write().await;
        crate::memory::write_memory(&mut graph, npc_id, &event);
        println!("wrote memory for {} (significance: {:.2})", npc_id, significance);
    }

    // persist
    {
        let graph = state.graph.read().await;
        if let Some(db) = &state.db {
            let db = db.lock().unwrap();
            if let Err(e) = crate::db::save_graph(&*db, &graph) {
                eprintln!("failed to persist after npc tick: {}", e);
            }
        }
    }

    // broadcast updated state
    let snapshot = crate::server::build_snapshot(&state).await;
    let _ = state.tx.send(snapshot);

    // return emitted signal if the NPC decided to act visibly
    Ok(emitted_signal.map(|(context, strength)| {
        crate::signal::EventSignal::new(npc_id, strength, &context)
    }))
}

fn build_npc_context(graph: &ESGraph, npc_id: &str, signal_context: &str, signal_strength: f64) -> String {
    let npc = match graph.nodes.get(npc_id) {
        Some(n) => n,
        None => return format!("NPC {} not found", npc_id),
    };

    let npc_name = npc_id.split(':').nth(1).unwrap_or(npc_id);
    let mut ctx = String::new();

    // NPC identity
    ctx.push_str("YOUR IDENTITY\n");
    ctx.push_str(&format!("id: {}\n", npc_id));
    for (k, v) in &npc.props {
        // skip transient signal props
        if k.starts_with("last_signal_") || k.starts_with("awareness_") { continue; }
        ctx.push_str(&format!("  {}: {}\n", k, format_value(v)));
    }

    // NPC stat block
    if let Some(stats_node) = crate::stats::get_stat_block(graph, npc_name) {
        ctx.push_str("\nYOUR STATS\n");
        for (k, v) in &stats_node.props {
            ctx.push_str(&format!("  {}: {}\n", k, format_value(v)));
        }
    }

    // NPC relationships
    if !npc.edges.is_empty() {
        ctx.push_str("\nYOUR RELATIONSHIPS\n");
        for edge in &npc.edges {
            ctx.push_str(&format!("  --[{}]--> {}:{}\n",
                edge.label, edge.target_type, edge.target_id));
        }
    }

    // what was perceived
    ctx.push_str("\nWHAT YOU PERCEIVED\n");
    ctx.push_str(&format!("  signal: {}\n", signal_context));
    ctx.push_str(&format!("  strength: {:.2}\n", signal_strength));

    // current awareness state
    let awareness = crate::stats::current_awareness(npc, graph);
    let perception = crate::stats::current_perception(npc, graph);
    ctx.push_str(&format!("  your_awareness: {:.2}\n", awareness));
    ctx.push_str(&format!("  your_perception: {:.2}\n", perception));

    // relevant memories
    let memories = crate::memory::get_relevant_memories(graph, npc_id, &signal_context, 5);
    if !memories.is_empty() {
        ctx.push_str("\nYOUR MEMORIES\n");
        for (_, node) in &memories {
            let action = match node.props.get("action") {
                Some(crate::graph::ESValue::Text(s)) => s.as_str(),
                _ => "unknown",
            };
            let subject = match node.props.get("subject") {
                Some(crate::graph::ESValue::Text(s)) => s.as_str(),
                _ => "unknown",
            };
            let significance = node.get_number("significance").unwrap_or(0.0);
            ctx.push_str(&format!("  - [sig {:.1}] {} (about {})\n", significance, action, subject));
        }
    }

    // nearby world state
    let npc_location = match npc.props.get("location") {
        Some(crate::graph::ESValue::Text(l)) => l.clone(),
        _ => String::from("unknown"),
    };

    ctx.push_str(&format!("\nYOUR LOCATION: {}\n", npc_location));
    ctx.push_str("\nNEARBY\n");

    for (id, node) in &graph.nodes {
        if id == npc_id { continue; }
        if !ESGraph::is_world_key(id) { continue; }
        if let Some(crate::graph::ESValue::Text(loc)) = node.props.get("location") {
            if loc == &npc_location {
                ctx.push_str(&format!("  {}\n", id));
                for (k, v) in &node.props {
                    if k.starts_with("last_signal_") || k.starts_with("awareness_") { continue; }
                    ctx.push_str(&format!("    {}: {}\n", k, format_value(v)));
                }
            }
        }
    }

    ctx
}

async fn call_npc_agent(context: &str, npc_name: &str) -> Result<String, String> { 
    
    let prompt = format!(
    r#"You are {name}, an NPC in a graph-based RPG world. You just perceived something happening nearby. Decide how you react based on your personality, role, and relationships.

    Respond with ONLY valid Edgescript. No explanation, no markdown, no code blocks.

    EDGESCRIPT SYNTAX
    @type:id                  — world node declaration
    @namespace/type:id        — namespaced node declaration
    key: value                — property (indented under its node)
    --[label]--> @type:id     — edge (indented under its node)

    Every node MUST have a colon between type and id. The colon is required.
    CORRECT: @npc:{name}
    CORRECT: @npc:guard
    WRONG:   @npc/{name}

    EXAMPLE PATCH
    @npc:{name}
        alert_level: high
        current_action: shouting for the guard
        narrative: "Saw the theft and is calling for help."
        signal_emit: "shouts: Stop, thief!"
        signal_strength: 0.8

    @npc:guard
        disposition: hostile

    RULES
    - Every edge MUST be directly under its node declaration, indented 2 spaces
    - NEVER write edges without a node declaration above them
    - You can ONLY write to world namespace nodes (no inventory/, equipped/, etc.)
    - NEVER write to stats/* — stats are system managed

    You MUST update your own node @npc:{name} with:
    alert_level: low/medium/high/hostile
    current_action: what you are doing right now
    narrative: brief description of your reaction

    If you want to do something noticeable (shout, attack, run, investigate), also set:
    signal_emit: brief description of what others would perceive
    signal_strength: 0.0 to 1.0 (how noticeable your action is)

    You may also update other world nodes if your action affects them (e.g. move to a new location, interact with objects). You may add new edges to represent new relationships.

    Output ONLY Edgescript. Nothing else.

    Context:
    {context}

    Edgescript patch:"#,
        name = npc_name,
        context = context
    );
    crate::llm::call_ollama(crate::llm::NPC_MODEL, &prompt).await
}