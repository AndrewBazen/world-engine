use crate::graph::{ESGraph, ESNode, ESValue};


pub struct MemoryEvent {
    pub event_id: String,
    pub subject: String,
    pub action: String,
    pub outcome: String,
    pub npc_response: String,
    pub location: String,
    pub timestamp: f64,
    pub significance: f64,
}

impl MemoryEvent {
    pub fn new(
        subject: String,
        action: String,
        outcome: String,
        npc_response: String,
        location: String,
        significance: f64,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        MemoryEvent {
            event_id: generate_event_id(),
            subject,
            action,
            outcome,
            npc_response,
            location,
            timestamp: now,
            significance,
        }
    }
}

// ── Write ───────────────────────────────────────────────────

/// Write a memory event too the graph as a node in memory/{npc_name}/
pub fn write_memory(graph: &mut ESGraph, npc_id: &str, event: &MemoryEvent) {
    // create node in memory/{npc_name}/event:{event_id}
    let name = npc_name(npc_id);
    let namespace = format!("memory/{}", name);
    let mut memory_node = ESNode::new(
        &namespace,
        "event",
        &event.event_id,
    );

    // insert all fields as props
    memory_node.props.insert("subject".to_string(), ESValue::Text(event.subject.clone()));
    memory_node.props.insert("action".to_string(), ESValue::Text(event.action.clone()));
    memory_node.props.insert("outcome".to_string(), ESValue::Text(event.outcome.clone()));
    memory_node.props.insert("npc_response".to_string(), ESValue::Text(event.npc_response.clone()));
    memory_node.props.insert("location".to_string(), ESValue::Text(event.location.clone()));
    memory_node.props.insert("timestamp".to_string(), ESValue::Number(event.timestamp));
    memory_node.props.insert("significance".to_string(), ESValue::Number(event.significance));

    graph.insert(memory_node);
}

// ── Query ───────────────────────────────────────────────────

/// Get all memories for an NPC, sorted by timestamp descending
pub fn get_memories<'a>(graph: &'a ESGraph, npc_id: &str) -> Vec<(&'a String, &'a ESNode)> {
    // filter graph.nodes by prefix "memory/{npc_name}/"
    let prefix = format!("memory/{}/", npc_name(npc_id));
    let mut memories: Vec<_> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .collect();
    
    memories.sort_by(|a, b| {
        let ts_a = a.1.get_number("timestamp").unwrap_or(0.0);
        let ts_b = b.1.get_number("timestamp").unwrap_or(0.0);
        ts_b.partial_cmp(&ts_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    memories
    // sort by timestamp descending
}

/// Get memories about a specific subject
pub fn get_memories_about<'a>(graph: &'a ESGraph, npc_id: &str, subject: &str) -> Vec<(&'a String, &'a ESNode)> {
    // filter get_memories by subject prop
    get_memories(graph, npc_id)
        .into_iter()
        .filter(|(_, node)| {
            matches!(node.props.get("subject"), Some(ESValue::Text(s)) if s == subject)
        })
        .collect()
}

/// Get the most relevant memories for building NPC context
/// Scores by: recency, significance, subject match
/// Returns up to `limit` memories
pub fn get_relevant_memories<'a>(
    graph: &'a ESGraph,
    npc_id: &str,
    subject: &str,
    limit: usize,
) -> Vec<(&'a String, &'a ESNode)> {
    // get all memories
    let mut memories = get_memories(graph, npc_id);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    memories.sort_by(|a, b| {
        let score_a = relevance_score(a.1, subject, now);
        let score_b = relevance_score(b.1, subject, now);
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    memories.into_iter().take(limit).collect()
}

fn relevance_score(node: &ESNode, subject: &str, now: f64) -> f64 {
    let significance = node.get_number("significance").unwrap_or(0.0);
    let timestamp = node.get_number("timestamp").unwrap_or(0.0);
    let age = (now - timestamp).max(0.0);
    let recency = (-0.001 * age).exp(); // decays over time

    let subject_match = match node.props.get("subject") {
        Some(ESValue::Text(s)) if s == subject => 1.0,
        _ => 0.0,
    };

    significance * 0.4 + recency * 0.3 + subject_match * 0.3
}

// ── Significance ────────────────────────────────────────────

/// Calculate significance mechanically from signal properties
pub fn calculate_significance(
    signal_strength: f64,
    is_direct_target: bool,
) -> f64 {
    // base significance from signal strength
    let mut sig = signal_strength;
    
    if is_direct_target {
        sig += 0.2;
    }
    
    // clamp 0.0 to 1.0
    sig.clamp(0.0, 1.0)
}

// ── Capacity ────────────────────────────────────────────────

/// How many memories this NPC currently has
pub fn memory_count(graph: &ESGraph, npc_id: &str) -> usize {
    // count nodes with prefix "memory/{npc_name}/"
    let prefix = format!("memory/{}/", npc_name(npc_id));
    graph.nodes.keys().filter(|k| k.starts_with(&prefix)).count()
}

/// Max memories based on intelligence stat
pub fn memory_capacity(graph: &ESGraph, npc_id: &str) -> usize {
    // 10 + intelligence stat
    let name = npc_name(npc_id);
    let intelligence = crate::stats::get_stat(graph, name, "intelligence");
    (10.0 + intelligence) as usize
}

/// Whether this NPC needs memory consolidation
pub fn needs_consolidation(graph: &ESGraph, npc_id: &str) -> bool {
    // memory_count >= memory_capacity * 0.8 or similar threshold
    let count = memory_count(graph, npc_id);
    let capacity = memory_capacity(graph, npc_id);
    count >= (capacity * 8 / 10)
}

// ── Pruning ─────────────────────────────────────────────────

/// Remove the lowest significance memories to free up space
pub fn prune_lowest(graph: &mut ESGraph, npc_id: &str, count: usize) {
    let prefix = format!("memory/{}/", npc_name(npc_id));
    // get all memories sorted by significance ascending
    let mut memories: Vec<(String, f64)> = graph.nodes.iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .filter(|(_, node)| {
            // skip consolidated memories
            !matches!(node.props.get("consolidated"), Some(ESValue::Bool(true)))
        })
        .map(|(k, node)| {
            let sig = node.get_number("significance").unwrap_or(0.0);
            (k.clone(), sig)
        })
        .collect();
    
    memories.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    // remove the least significant ones
    for (key, _) in memories.into_iter().take(count) {
        graph.nodes.remove(&key);
    }
}

// ── Helpers ─────────────────────────────────────────────────

/// Generate a unique event ID from timestamp
fn generate_event_id() -> String {
    // use current timestamp millis for uniqueness
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    format!("ev_{}", now)
}

/// Extract the npc name from an npc_id like "npc:guard" -> "guard"
fn npc_name(npc_id: &str) -> &str {
    npc_id.split(':').nth(1).unwrap_or(npc_id)
}