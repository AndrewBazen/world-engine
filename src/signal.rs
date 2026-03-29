use std::collections::HashSet;
use std::sync::Arc;
use std::collections::VecDeque;
use crate::graph::{ESGraph, ESNode, ESValue};
use crate::server::ServerMessage;
use crate::state::AppState;

pub const DISSIPATION_THRESHOLD: f64 = 0.05;
pub const DECAY_FACTOR: f64 = 0.7;

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

pub async fn propagate(state: Arc<AppState>, initial_signal: EventSignal) {
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

        let mut next_signals = Vec::new();

        for edge in &node.edges {
            let neighbor_id = format!("{}:{}", edge.target_type, edge.target_id);

            if signal.visited.contains(&neighbor_id) { continue; }

            let arriving = signal.strength * edge.affinity;
            if arriving < DISSIPATION_THRESHOLD { continue; }

            let should_absorb = {
                let graph = state.graph.read().await;
                graph.nodes.get(&neighbor_id)
                    .map(|n| n.should_absorb(&signal, arriving))
                    .unwrap_or(false)
            };

            let _ = state.tx.send(ServerMessage::SignalHop {
                from: signal.origin_id.clone(),
                to: neighbor_id.clone(),
                strength: arriving,
                context: signal.context.clone(),
                absorbed: should_absorb,
            });

            if should_absorb {
                {
                    let mut graph = state.graph.write().await;
                    if let Some(neighbor) = graph.nodes.get_mut(&neighbor_id) {
                        neighbor.absorb(&signal, arriving);

                        // broadcast updated props after absorption
                        let props = serde_json::to_value(&neighbor.props).unwrap_or_default();
                        let _ = state.tx.send(ServerMessage::NodeUpdate { 
                            id: neighbor_id.clone(), 
                            props,
                        });
                    }
                }

                let mut next = EventSignal {
                    origin_id: neighbor_id.clone(),
                    strength: arriving * DECAY_FACTOR,
                    context: signal.context.clone(),
                    visited: signal.visited.clone(),
                };
                next.visited.insert(neighbor_id);
                next_signals.push(next);
            }
        }

        // wait after each hop ring before expanding further
        tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;

        // add next hop signals to queue
        for next in next_signals {
            queue.push_back(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_absorb() {
        let mut node = ESNode::new("world", "npc", "guard")
            .with_prop("threshold", ESValue::Number(0.4))
            .with_prop("activation", ESValue::Number(0.0));

        let signal = EventSignal::new("player:andrew", 0.8, "slipped past the garrison unseen");

        assert!(node.should_absorb(&signal, 0.8));   // above threshold
        assert!(!node.should_absorb(&signal, 0.2));  // below threshold

        node.absorb(&signal, 0.8);

        assert!(matches!(
            node.props.get("activation"),
            Some(ESValue::Number(v)) if *v > 0.0
        ));
        assert!(matches!(
            node.props.get("last_signal_context"),
            Some(ESValue::Text(s)) if s == "slipped past the garrison unseen"
        ));
    }

    #[tokio::test]
    async fn test_signal_propagation() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("world", "player", "andrew")
            .with_prop("threshold", ESValue::Number(0.1))
            .with_edge("near", "npc", "guard");

        let guard = ESNode::new("world", "npc", "guard")
            .with_prop("threshold", ESValue::Number(0.4))
            .with_prop("activation", ESValue::Number(0.0))
            .with_edge("reports_to", "npc", "commander");

        let commander = ESNode::new("world", "npc", "commander")
            .with_prop("threshold", ESValue::Number(0.3))
            .with_prop("activation", ESValue::Number(0.0));

        graph.insert(player);
        graph.insert(guard);
        graph.insert(commander);

        // graph moves into state here — use state for everything after
        let state = AppState::new(graph);

        let signal = EventSignal::new(
            "player:andrew",
            0.9,
            "slipped past the garrison unseen"
        );

        propagate(state.clone(), signal).await;

        // read graph through state after propagation
        let graph = state.graph.read().await;

        let guard = graph.get("world", "npc", "guard").unwrap();
        assert!(guard.get_number("activation").unwrap_or(0.0) > 0.0);

        let commander = graph.get("world", "npc", "commander").unwrap();
        assert!(commander.get_number("activation").unwrap_or(0.0) > 0.0);

        let guard_activation = graph.get("world", "npc", "guard")
            .unwrap()
            .get_number("activation")
            .unwrap();
        let commander_activation = graph.get("world", "npc", "commander")
            .unwrap()
            .get_number("activation")
            .unwrap();

        assert!(guard_activation > commander_activation);
    }
}