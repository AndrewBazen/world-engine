use std::collections::HashSet;
use crate::graph::{ESGraph, ESNode, ESValue};

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

pub fn propagate(graph: &mut ESGraph, signal: EventSignal) {
    let node = match graph.get_by_key(&signal.origin_id) {
        Some(n) => n.clone(),
        None => return,
    };

    for edge in &node.edges {
        let neighbor_id = format!("{}:{}", edge.target_type, edge.target_id);

        if signal.visited.contains(&neighbor_id) { continue; }

        let arriving = signal.strength * edge.affinity;

        if arriving < DISSIPATION_THRESHOLD { continue; }

        let should_absorb = graph
            .get_by_key(&neighbor_id)
            .map(|n| n.should_absorb(&signal, arriving))
            .unwrap_or(false);

        if should_absorb {
            if let Some(neighbor) = graph.get_mut(&neighbor_id) {
                neighbor.absorb(&signal, arriving);
            }

            let mut next_signal = EventSignal {
                origin_id: neighbor_id.clone(),
                strength: arriving * DECAY_FACTOR,
                context: signal.context.clone(),
                visited: signal.visited.clone(),
            };
            next_signal.visited.insert(neighbor_id);
            propagate(graph, next_signal);
        }
    }
}