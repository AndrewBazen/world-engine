use std::collections::HashSet;
use std::sync::Arc;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use crate::graph::{ESGraph, ESNode, ESValue};
use crate::server::ServerMessage;
use crate::state::AppState;
use crate::stats;

pub const DISSIPATION_THRESHOLD: f64 = 0.05;
pub const DECAY_FACTOR: f64 = 0.7;
pub const AMBIENT_DECAY: f64 = 0.7;

pub struct EventSignal {
    pub origin_id: String,
    pub strength: f64,
    pub context: String,
    pub visited: HashSet<String>,
    /// How many hops from the origin. Sent to clients so they can pace the
    /// animation themselves instead of the engine sleeping to do it for them.
    pub depth: u32,
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
            depth: 0,
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
            depth: 0,
        }
    }
}

/// An NPC that absorbed a signal and needs an agent decision call.
#[derive(Debug, Clone)]
pub struct AbsorbedSignal {
    pub npc_id: String,
    pub origin_id: String,
    pub context: String,
    pub strength: f64,
    pub turn: u64,
}

// ── Perception gate ──────────────────────────────────────────

/// Stat-derived perception check. Higher perception → detect weaker signals.
/// Returns true if the NPC perceives the signal.
fn perceives(node: &ESNode, graph: &ESGraph, arrival_strength: f64, turn: u64) -> bool {
    // only npcs percieve the signals
    if node.node_type != "npc" { return false; }

    let perception = stats::current_perception(node, graph, turn);
    // perception 0.8 → threshold 0.2 (catches weak signals)
    // perception 0.3 → threshold 0.7 (only catches strong signals)
    let threshold = 1.0 - perception;
    arrival_strength >= threshold
}

// ── Absorption ───────────────────────────────────────────────

/// Record that this NPC perceived a signal. Updates awareness state
/// so future perception checks reflect heightened alertness.
fn absorb(node: &mut ESNode, baseline: f64, current_awareness: f64, signal: &EventSignal, strength: f64, turn: u64) {
    // record what was perceived
    node.props.insert(
        "last_signal_context".to_string(),
        ESValue::Text(signal.context.clone()),
    );
    node.props.insert(
        "last_signal_strength".to_string(),
        ESValue::Number(strength),
    );

    // Raise awareness toward the level this event justifies — saturating, not
    // accumulating. The old form added `strength * 0.3` to the CURRENT value on
    // every absorption, so four signals in one turn ratcheted an NPC from 0.50
    // to 0.99 and left them with a threshold near zero, absorbing everything
    // for the next few minutes. Alertness should reflect how loud the loudest
    // thing was, not how many things there were.
    let alerted_to = baseline + (1.0 - baseline) * strength;
    let new_peak = current_awareness.max(alerted_to).min(1.0).max(baseline);

    node.props.insert(
        "awareness_peak".to_string(),
        ESValue::Number(new_peak),
    );

    node.props.insert(
        "awareness_last_raised".to_string(),
        ESValue::Number(turn as f64),
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
    let turn = state.turn.load(Ordering::Relaxed);

    // only propagate from world nodes
    if !ESGraph::is_world_key(&initial_signal.origin_id) {
        return (Vec::new(), HashSet::new());
    }

    // ONE visited set for the whole traversal.
    //
    // This used to be cloned into every continuation, so a node could be
    // reached again down every other path. That is quadratic on its own — but
    // absorbing also RAISES awareness, which lowers the threshold, so an NPC
    // re-absorbed its own echo arriving back from a neighbour, queued another
    // continuation, and the frontier grew instead of shrinking. Combined with a
    // sleep per dequeue it turned one action into minutes of wall clock.
    let mut all_visited: HashSet<String> = initial_signal.visited.clone();
    all_visited.insert(initial_signal.origin_id.clone());

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
            if all_visited.contains(&neighbor_id) { continue; }
            if !ESGraph::is_world_key(&neighbor_id) { continue; }

            let arriving = signal.strength * edge.affinity;
            if arriving < DISSIPATION_THRESHOLD { continue; }

            let neighbor_state = {
                let graph = state.graph.read().await;
                graph.nodes.get(&neighbor_id).map(|n| {
                    (
                        n.node_type == "npc",
                        perceives(n, &graph, arriving, turn),
                        node_location(n),
                        stats::get_baseline_awareness(n, &graph),
                        stats::current_awareness(n, &graph, turn),
                    )
                })
            };

            // A signal cannot travel through something that is not there.
            // Dangling edges are common — an agent writes an edge to a node it
            // never created, or the target was removed — and treating "missing"
            // as "not an NPC" sent a transit hop to a node the visualizer has
            // never heard of, every single propagation, forever.
            let Some((is_npc, perceived, neighbor_location, baseline, awareness)) = neighbor_state
            else {
                continue;
            };

            if is_npc && perceived {
                let _ = state.tx.send(ServerMessage::SignalHop {
                    from: signal.origin_id.clone(),
                    to: neighbor_id.clone(),
                    strength: arriving,
                    context: signal.context.clone(),
                    absorbed: perceived,
                    ambient: false,
                    transit: false,
                    hop: signal.depth + 1,
                });

                // absorb and update awareness
                {
                    let mut graph = state.graph.write().await;
                    if let Some(neighbor) = graph.nodes.get_mut(&neighbor_id) {
                        absorb(neighbor, baseline, awareness, &signal, arriving, turn);

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
                    origin_id: signal.origin_id.clone(),
                    context: signal.context.clone(),
                    strength: arriving,
                    turn: turn,
                });

                // queue continuation along this node's edges
                all_visited.insert(neighbor_id.clone());
                next_signals.push(EventSignal {
                    origin_id: neighbor_id,
                    strength: arriving * DECAY_FACTOR,
                    context: signal.context.clone(),
                    visited: HashSet::new(), // the traversal-wide set is authoritative
                    depth: signal.depth + 1,
                });
            } else if !is_npc {
                // A place, item or faction. It never absorbs, but it carries
                // the signal onward — emit a hop so the route is visible
                // instead of pulses appearing out of nowhere.
                let _ = state.tx.send(ServerMessage::SignalHop {
                    from: signal.origin_id.clone(),
                    to: neighbor_id.clone(),
                    strength: arriving,
                    context: signal.context.clone(),
                    absorbed: false,
                    ambient: false,
                    transit: true,
                    hop: signal.depth + 1,
                });

                // queue continuation along this node's edges
                all_visited.insert(neighbor_id.clone());
                next_signals.push(EventSignal {
                    origin_id: neighbor_id,
                    strength: arriving * DECAY_FACTOR,
                    context: signal.context.clone(),
                    visited: HashSet::new(),
                    depth: signal.depth + 1,
                });
            } else {
                // An NPC that failed its perception check. The signal stops
                // here, and that miss is worth seeing. Mark it visited so the
                // same miss is not re-reported from every other direction.
                let _ = state.tx.send(ServerMessage::SignalHop {
                    from: signal.origin_id.clone(),
                    to: neighbor_id.clone(),
                    strength: arriving,
                    context: signal.context.clone(),
                    absorbed: false,
                    ambient: false,
                    transit: false,
                    hop: signal.depth + 1,
                });
                all_visited.insert(neighbor_id);
            }
        }

        // ── Phase 2: ambient broadcast at touched locations ──────

        for location in &locations_touched {
            let ambient_strength = signal.strength * AMBIENT_DECAY;
            if ambient_strength < DISSIPATION_THRESHOLD { continue; }

            let nearby_npcs = {
                let graph = state.graph.read().await;
                npcs_at_location(&graph, location, &all_visited)
            };

            for npc_id in &nearby_npcs {
                let (is_npc, perceived, baseline, awareness) = {
                    let graph = state.graph.read().await;
                    match graph.nodes.get(npc_id) {
                        Some(n) => (
                            n.node_type == "npc",
                            perceives(n, &graph, ambient_strength, turn),
                            stats::get_baseline_awareness(n, &graph),
                            stats::current_awareness(n, &graph, turn),
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
                        transit: false,
                        hop: signal.depth + 1,
                    });

                    {
                        let mut graph = state.graph.write().await;
                        if let Some(npc_node) = graph.nodes.get_mut(npc_id) {
                            absorb(npc_node, baseline, awareness, &signal, ambient_strength, turn);

                            let props = serde_json::to_value(&npc_node.props).unwrap_or_default();
                            let _ = state.tx.send(ServerMessage::NodeUpdate {
                                id: npc_id.clone(),
                                props,
                            });
                        }
                    }

                    absorbed_npcs.push(AbsorbedSignal {
                        npc_id: npc_id.clone(),
                        origin_id: signal.origin_id.clone(),
                        context: signal.context.clone(),
                        strength: ambient_strength,
                        turn: turn,
                    });

                    // ambient-absorbed NPCs also propagate structurally
                    all_visited.insert(npc_id.clone());
                    next_signals.push(EventSignal {
                        origin_id: npc_id.clone(),
                        strength: ambient_strength * DECAY_FACTOR,
                        context: signal.context.clone(),
                        visited: HashSet::new(),
                        depth: signal.depth + 1,
                    });
                }
            }
        }

        // No sleep here. Pacing the animation used to be done by stalling the
        // simulation 350ms per DEQUEUED NODE — not per ring, despite the old
        // comment — so a twenty node frontier cost seven seconds inside the
        // world model, before a single NPC agent had even been called. Hops now
        // carry their ring index and the client schedules its own animation.
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
