use std::collections::HashSet;
use std::sync::Arc;
use std::collections::VecDeque;
use crate::graph::{ESGraph, ESNode, ESValue};
use crate::server::ServerMessage;
use crate::state::AppState;
use crate::stats;

pub const DISSIPATION_THRESHOLD: f64 = 0.05;
pub const DECAY_FACTOR: f64 = 0.7;
pub const AMBIENT_DECAY: f64 = 0.5;

pub struct EventSignal {
    pub origin_id: String,
    pub strength: f64,
    pub context: String,
    pub visited: HashSet<String>,
}

impl EventSignal {
    pub fn new(origin_id: &str, strength: f64, context: &str) -> Self {
        let mut visited = HashSet::new();
        visited.insert(origin_id.to_string());
        EventSignal {
            origin_id: origin_id.to_string(),
            strength,
            context: context.to_string(),
            visited,
        }
    }
}

impl EventSignal {
    pub fn with_visited(origin_id: &str, strength: f64, context: &str, visited: HashSet<String>) -> Self {
        let mut visited = visited;
        visited.insert(origin_id.to_string());
        EventSignal {
            origin_id: origin_id.to_string(),
            strength,
            context: context.to_string(),
            visited,
        }
    }
}

/// An NPC that absorbed a signal and needs an agent decision call.
#[derive(Debug, Clone)]
pub struct AbsorbedSignal {
    pub npc_id: String,
    pub context: String,
    pub strength: f64,
}

// ── Perception gate ──────────────────────────────────────────

/// Stat-derived perception check. Higher perception → detect weaker signals.
/// Returns true if the NPC perceives the signal.
fn perceives(node: &ESNode, graph: &ESGraph, arrival_strength: f64) -> bool {
    // only npcs percieve the signals
    if node.node_type != "npc" { return false; }

    let perception = stats::current_perception(node, graph);
    // perception 0.8 → threshold 0.2 (catches weak signals)
    // perception 0.3 → threshold 0.7 (only catches strong signals)
    let threshold = 1.0 - perception;
    arrival_strength >= threshold
}

// ── Absorption ───────────────────────────────────────────────

/// Record that this NPC perceived a signal. Updates awareness state
/// so future perception checks reflect heightened alertness.
fn absorb(node: &mut ESNode, baseline: f64, current_awareness: f64, signal: &EventSignal, strength: f64) {
    // record what was perceived
    node.props.insert(
        "last_signal_context".to_string(),
        ESValue::Text(signal.context.clone()),
    );
    node.props.insert(
        "last_signal_strength".to_string(),
        ESValue::Number(strength),
    );

    // raise awareness — perception checks use this for heightened alertness
    let new_peak = (current_awareness + strength * 0.3).min(1.0).max(baseline);

    node.props.insert(
        "awareness_peak".to_string(),
        ESValue::Number(new_peak),
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    node.props.insert(
        "awareness_last_raised".to_string(),
        ESValue::Number(now),
    );
}

// ── Ambient broadcast ────────────────────────────────────────

/// Find all world-namespace NPC nodes at a given location.
fn npcs_at_location(graph: &ESGraph, location: &str, exclude: &HashSet<String>) -> Vec<String> {
    graph.nodes.iter()
        .filter(|(id, _)| ESGraph::is_world_key(id))
        .filter(|(id, _)| !exclude.contains(*id))
        .filter(|(_, node)| node.node_type == "npc")
        .filter(|(_, node)| {
            matches!(node.props.get("location"), Some(ESValue::Text(loc)) if loc == location)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Get the location of a node, if it has one.
fn node_location(node: &ESNode) -> Option<String> {
    match node.props.get("location") {
        Some(ESValue::Text(loc)) => Some(loc.clone()),
        _ => None,
    }
}

// ── Propagation ──────────────────────────────────────────────

/// Propagate a signal through the world graph.
///
/// Phase 1 (structural): walk explicit edges, perception-gated absorption.
/// Phase 2 (ambient): at each location touched, check nearby NPCs.
///
/// Returns a list of NPCs that absorbed the signal and need agent calls.
pub async fn propagate(state: Arc<AppState>, initial_signal: EventSignal) -> (Vec<AbsorbedSignal>, HashSet<String>) {
    let mut all_visited: HashSet<String> = HashSet::new();
    all_visited.insert(initial_signal.origin_id.clone());
    // only propagate from world nodes
    if !ESGraph::is_world_key(&initial_signal.origin_id) {
        return (Vec::new(), HashSet::new());
    }

    let mut absorbed_npcs: Vec<AbsorbedSignal> = Vec::new();
    let mut queue: VecDeque<EventSignal> = VecDeque::new();
    queue.push_back(initial_signal);

    while let Some(signal) = queue.pop_front() {
        let node = {
            let graph = state.graph.read().await;
            match graph.nodes.get(&signal.origin_id) {
                Some(n) => n.clone(),
                None => continue,
            }
        };

        let mut next_signals: Vec<EventSignal> = Vec::new();
        let mut locations_touched: HashSet<String> = HashSet::new();

        // if the origin node has a location, it's touched
        if let Some(loc) = node_location(&node) {
            locations_touched.insert(loc);
        }

        // ── Phase 1: structural propagation along explicit edges ──
        for edge in &node.edges {
            let neighbor_id = format!("{}:{}", edge.target_type, edge.target_id);

            // skip visited, private namespace, and below-threshold
            if signal.visited.contains(&neighbor_id) { continue; }
            if !ESGraph::is_world_key(&neighbor_id) { continue; }

            let arriving = signal.strength * edge.affinity;
            if arriving < DISSIPATION_THRESHOLD { continue; }

            let (is_npc, perceived, neighbor_location, baseline, awareness) = {
                let graph = state.graph.read().await;
                match graph.nodes.get(&neighbor_id) {
                    Some(n) => (
                        n.node_type == "npc",
                        perceives(n, &graph, arriving),
                        node_location(n),
                        stats::get_baseline_awareness(n, &graph),
                        stats::current_awareness(n, &graph),
                    ),
                    None => (false, false, None, 0.0, 0.0),
                }
            };


            if is_npc && perceived {
                let _ = state.tx.send(ServerMessage::SignalHop {
                    from: signal.origin_id.clone(),
                    to: neighbor_id.clone(),
                    strength: arriving,
                    context: signal.context.clone(),
                    absorbed: perceived,
                    ambient: false,
                });

                // absorb and update awareness
                {
                    let mut graph = state.graph.write().await;
                    if let Some(neighbor) = graph.nodes.get_mut(&neighbor_id) {
                        absorb(neighbor, baseline, awareness, &signal, arriving);

                        let props = serde_json::to_value(&neighbor.props).unwrap_or_default();
                        let _ = state.tx.send(ServerMessage::NodeUpdate {
                            id: neighbor_id.clone(),
                            props,
                        });
                    }
                }

                // track location for ambient broadcast
                if let Some(loc) = neighbor_location {
                    locations_touched.insert(loc);
                }

                // queue for NPC agent call if this is an NPC
                absorbed_npcs.push(AbsorbedSignal {
                    npc_id: neighbor_id.clone(),
                    context: signal.context.clone(),
                    strength: arriving,
                });

                // queue continuation along this node's edges
                let mut next = EventSignal {
                    origin_id: neighbor_id.clone(),
                    strength: arriving * DECAY_FACTOR,
                    context: signal.context.clone(),
                    visited: signal.visited.clone(),
                };
                all_visited.insert(neighbor_id.clone());
                next.visited.insert(neighbor_id);
                next_signals.push(next);
            } else if !is_npc {
                // queue continuation along this node's edges
                let mut next = EventSignal {
                    origin_id: neighbor_id.clone(),
                    strength: arriving * DECAY_FACTOR,
                    context: signal.context.clone(),
                    visited: signal.visited.clone(),
                };
                all_visited.insert(neighbor_id.clone());
                next.visited.insert(neighbor_id);
                next_signals.push(next);
            }
        }

        // ── Phase 2: ambient broadcast at touched locations ──────

        for location in &locations_touched {
            let ambient_strength = signal.strength * AMBIENT_DECAY;
            if ambient_strength < DISSIPATION_THRESHOLD { continue; }

            let nearby_npcs = {
                let graph = state.graph.read().await;
                npcs_at_location(&graph, location, &signal.visited)
            };

            for npc_id in &nearby_npcs {
                let (is_npc, perceived, baseline, awareness) = {
                    let graph = state.graph.read().await;
                    match graph.nodes.get(npc_id) {
                        Some(n) => (
                            n.node_type == "npc",
                            perceives(n, &graph, ambient_strength),
                            stats::get_baseline_awareness(n, &graph),
                            stats::current_awareness(n, &graph),
                        ),
                        None => (false, false, 0.0, 0.0),
                    }
                };

                if is_npc && perceived {
                    let _ = state.tx.send(ServerMessage::SignalHop {
                        from: signal.origin_id.clone(),
                        to: npc_id.clone(),
                        strength: ambient_strength,
                        context: signal.context.clone(),
                        absorbed: true,
                        ambient: true,
                    });


                    {
                        let mut graph = state.graph.write().await;
                        if let Some(npc_node) = graph.nodes.get_mut(npc_id) {
                            absorb(npc_node, baseline, awareness, &signal, ambient_strength);

                            let props = serde_json::to_value(&npc_node.props).unwrap_or_default();
                            let _ = state.tx.send(ServerMessage::NodeUpdate {
                                id: npc_id.clone(),
                                props,
                            });
                        }
                    }

                    absorbed_npcs.push(AbsorbedSignal {
                        npc_id: npc_id.clone(),
                        context: signal.context.clone(),
                        strength: ambient_strength,
                    });

                    // ambient-absorbed NPCs also propagate structurally
                    let mut next = EventSignal {
                        origin_id: npc_id.clone(),
                        strength: ambient_strength * DECAY_FACTOR,
                        context: signal.context.clone(),
                        visited: signal.visited.clone(),
                    };
                    all_visited.insert(npc_id.clone());
                    next.visited.insert(npc_id.clone());
                    next_signals.push(next);
                }
            }
        }

        // wait after each hop ring before expanding further
        tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;

        for next in next_signals {
            queue.push_back(next);
        }
    }

    // deduplicate — if an NPC absorbed multiple times, keep the strongest
    absorbed_npcs.sort_by(|a, b| a.npc_id.cmp(&b.npc_id));
    absorbed_npcs.dedup_by(|a, b| {
        if a.npc_id == b.npc_id {
            // keep the one with higher strength in b (b survives dedup)
            if a.strength > b.strength {
                b.strength = a.strength;
                b.context = a.context.clone();
            }
            true
        } else {
            false
        }
    });

    (absorbed_npcs, all_visited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_structural_propagation_with_perception() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_edge("near", "npc", "guard");

        // guard needs a stat block for perception to work
        let guard = ESNode::new("world", "npc", "guard")
            .with_edge("reports_to", "npc", "commander");

        let commander = ESNode::new("world", "npc", "commander");

        graph.insert(player);
        graph.insert(guard);
        graph.insert(commander);

        // give both NPCs stat blocks so perception works
        let guard_stats = stats::StatBlock::default();
        stats::write_stat_block(&mut graph, "guard", &guard_stats);

        let commander_stats = stats::StatBlock::default();
        stats::write_stat_block(&mut graph, "commander", &commander_stats);

        let state = AppState::new_without_db(graph);

        let signal = EventSignal::new(
            "player:andrew",
            0.9,
            "slipped past the garrison unseen",
        );

        let (absorbed, visited) = crate::signal::propagate(state.clone(), signal).await;

        // guard should have absorbed (strong signal, default perception)
        assert!(absorbed.iter().any(|a| a.npc_id == "npc:guard"));

        // check awareness was updated (not activation)
        let graph = state.graph.read().await;
        let guard = graph.get("world", "npc", "guard").unwrap();
        assert!(guard.get_number("awareness_peak").is_some());
        assert!(guard.get_number("awareness_last_raised").is_some());
        assert!(matches!(
            guard.props.get("last_signal_context"),
            Some(ESValue::Text(s)) if s == "slipped past the garrison unseen"
        ));
    }

    #[tokio::test]
    async fn test_ambient_broadcast() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string()))
            .with_edge("near", "npc", "merchant");

        // merchant is connected by edge
        let merchant = ESNode::new("world", "npc", "merchant")
            .with_prop("location", ESValue::Text("market".to_string()));

        // bystander has no edge to player but is at same location
        let bystander = ESNode::new("world", "npc", "bystander")
            .with_prop("location", ESValue::Text("market".to_string()));

        // distant_guard is at a different location — should NOT absorb
        let distant_guard = ESNode::new("world", "npc", "distant_guard")
            .with_prop("location", ESValue::Text("barracks".to_string()));

        graph.insert(player);
        graph.insert(merchant);
        graph.insert(bystander);
        graph.insert(distant_guard);

        // stat blocks for all NPCs
        let default_stats = stats::StatBlock::default();
        stats::write_stat_block(&mut graph, "merchant", &default_stats);
        stats::write_stat_block(&mut graph, "bystander", &default_stats);
        stats::write_stat_block(&mut graph, "distant_guard", &default_stats);

        let state = AppState::new_without_db(graph);

        let signal = EventSignal::new(
            "player:andrew",
            0.9,
            "stole bread from merchant stall",
        );

        let (absorbed, visited) = crate::signal::propagate(state.clone(), signal).await;

        // merchant absorbed via structural edge
        assert!(absorbed.iter().any(|a| a.npc_id == "npc:merchant"));
        // bystander absorbed via ambient broadcast (same location)
        assert!(absorbed.iter().any(|a| a.npc_id == "npc:bystander"));
        // distant_guard should NOT have absorbed
        assert!(!absorbed.iter().any(|a| a.npc_id == "npc:distant_guard"));
    }

    #[tokio::test]
    async fn test_weak_signal_filtered_by_perception() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_edge("near", "npc", "dim_guard");

        let dim_guard = ESNode::new("world", "npc", "dim_guard");

        graph.insert(player);
        graph.insert(dim_guard);

        // give dim_guard a very low perception stat block
        let mut low_stats = stats::StatBlock::default();
        low_stats.wisdom = 3;
        low_stats.skills.perception = -2;
        let low_stats = low_stats.clamp();
        stats::write_stat_block(&mut graph, "dim_guard", &low_stats);

        let state = AppState::new_without_db(graph);

        // weak signal — should fail perception check for a dim guard
        let signal = EventSignal::new(
            "player:andrew",
            0.3,
            "quietly pocketed a coin",
        );

        let (absorbed, visited) = crate::signal::propagate(state.clone(), signal).await;

        // dim guard should NOT perceive a weak signal
        assert!(!absorbed.iter().any(|a| a.npc_id == "npc:dim_guard"));
    }

    #[tokio::test]
    async fn test_propagation_skips_private_nodes() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string()))
            .with_edge("near", "npc", "guard");

        let item = ESNode::new("inventory/andrew", "item", "sword")
            .with_prop("location", ESValue::Text("market".to_string()));

        let guard = ESNode::new("world", "npc", "guard")
            .with_prop("location", ESValue::Text("market".to_string()));

        graph.insert(player);
        graph.insert(item);
        graph.insert(guard);

        let default_stats = stats::StatBlock::default();
        stats::write_stat_block(&mut graph, "guard", &default_stats);

        let state = AppState::new_without_db(graph);

        let signal = EventSignal::new(
            "player:andrew",
            0.9,
            "drew a weapon",
        );

        let (absorbed, visited) = crate::signal::propagate(state.clone(), signal).await;

        // guard should absorb, but no inventory items should appear
        assert!(absorbed.iter().any(|a| a.npc_id == "npc:guard"));
        assert!(!absorbed.iter().any(|a| a.npc_id.contains("inventory")));
    }
}