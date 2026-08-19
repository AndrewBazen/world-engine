use super::*;
use crate::graph::{ESGraph, ESNode, ESValue};
use crate::state::AppState;
use crate::stats;

#[tokio::test]
async fn test_structural_propagation_with_perception() {
    let mut graph = ESGraph::new();

    graph.insert(ESNode::new("world", "player", "andrew").with_edge("near", "npc", "guard"));
    graph.insert(ESNode::new("world", "npc", "guard").with_edge("reports_to", "npc", "commander"));
    graph.insert(ESNode::new("world", "npc", "commander"));

    // both NPCs need stat blocks for perception to resolve
    stats::write_stat_block(&mut graph, "guard", &stats::StatBlock::default());
    stats::write_stat_block(&mut graph, "commander", &stats::StatBlock::default());

    let state = AppState::new_without_db(graph);

    let signal = EventSignal::new("player:andrew", 0.9, "slipped past the garrison unseen");
    let (absorbed, _) = propagate(state.clone(), signal).await;

    assert!(absorbed.iter().any(|a| a.npc_id == "npc:guard"));

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

    graph.insert(
        ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string()))
            .with_edge("near", "npc", "merchant"),
    );
    // connected by an edge
    graph.insert(
        ESNode::new("world", "npc", "merchant")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    // no edge to the player, but standing in the same place
    graph.insert(
        ESNode::new("world", "npc", "bystander")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    // somewhere else entirely
    graph.insert(
        ESNode::new("world", "npc", "distant_guard")
            .with_prop("location", ESValue::Text("barracks".to_string())),
    );

    for npc in ["merchant", "bystander", "distant_guard"] {
        stats::write_stat_block(&mut graph, npc, &stats::StatBlock::default());
    }

    let state = AppState::new_without_db(graph);

    let signal = EventSignal::new("player:andrew", 0.9, "stole bread from merchant stall");
    let (absorbed, _) = propagate(state.clone(), signal).await;

    assert!(absorbed.iter().any(|a| a.npc_id == "npc:merchant"), "reached by edge");
    assert!(absorbed.iter().any(|a| a.npc_id == "npc:bystander"), "reached by location");
    assert!(
        !absorbed.iter().any(|a| a.npc_id == "npc:distant_guard"),
        "a different district must not hear it"
    );
}

#[tokio::test]
async fn test_weak_signal_filtered_by_perception() {
    let mut graph = ESGraph::new();

    graph.insert(ESNode::new("world", "player", "andrew").with_edge("near", "npc", "dim_guard"));
    graph.insert(ESNode::new("world", "npc", "dim_guard"));

    let mut low = stats::StatBlock::default();
    low.wisdom = 3;
    low.skills.perception = -2;
    stats::write_stat_block(&mut graph, "dim_guard", &low.clamp());

    let state = AppState::new_without_db(graph);

    let signal = EventSignal::new("player:andrew", 0.3, "quietly pocketed a coin");
    let (absorbed, _) = propagate(state.clone(), signal).await;

    assert!(!absorbed.iter().any(|a| a.npc_id == "npc:dim_guard"));
}

#[tokio::test]
async fn test_propagation_skips_private_nodes() {
    let mut graph = ESGraph::new();

    graph.insert(
        ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string()))
            .with_edge("near", "npc", "guard"),
    );
    graph.insert(
        ESNode::new("inventory/andrew", "item", "sword")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    graph.insert(
        ESNode::new("world", "npc", "guard")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    stats::write_stat_block(&mut graph, "guard", &stats::StatBlock::default());

    let state = AppState::new_without_db(graph);

    let signal = EventSignal::new("player:andrew", 0.9, "drew a weapon");
    let (absorbed, _) = propagate(state.clone(), signal).await;

    assert!(absorbed.iter().any(|a| a.npc_id == "npc:guard"));
    assert!(!absorbed.iter().any(|a| a.npc_id.contains("inventory")));
}

#[tokio::test]
async fn test_dangling_edges_emit_no_hops() {
    let mut graph = ESGraph::new();
    graph.insert(
        ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string()))
            // neither of these targets exists
            .with_edge("saw", "npc", "ghost")
            .with_edge("entered", "location", "nowhere"),
    );

    let state = AppState::new_without_db(graph);
    let mut rx = state.tx.subscribe();

    let signal = EventSignal::new("player:andrew", 0.9, "looks around");
    let _ = propagate(state.clone(), signal).await;

    let mut targets = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let crate::server::ServerMessage::SignalHop { to, .. } = msg {
            targets.push(to);
        }
    }

    assert!(
        targets.is_empty(),
        "a signal must not travel through nodes that do not exist, got {:?}",
        targets
    );
}

#[tokio::test]
async fn test_awareness_saturates_instead_of_ratcheting() {
    let mut graph = ESGraph::new();
    graph.insert(
        ESNode::new("world", "player", "andrew")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    graph.insert(
        ESNode::new("world", "npc", "guard")
            .with_prop("location", ESValue::Text("market".to_string())),
    );
    stats::write_stat_block(&mut graph, "guard", &stats::StatBlock::default());

    let state = AppState::new_without_db(graph);

    // the same event, over and over
    for _ in 0..5 {
        let signal = EventSignal::new("player:andrew", 0.8, "a scuffle in the market");
        let _ = propagate(state.clone(), signal).await;
    }

    let graph = state.graph.read().await;
    let guard = graph.get("world", "npc", "guard").unwrap();
    let peak = guard.get_number("awareness_peak").unwrap();

    assert!(
        peak < 0.85,
        "repeated identical signals must not ratchet awareness toward 1.0 \
         (an NPC at 0.99 has a threshold near zero and absorbs everything), got {}",
        peak
    );
}
