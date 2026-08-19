use super::*;
use crate::graph::{ESGraph, ESNode, ESValue};


fn npc_with_passive(passive: f64) -> (ESGraph, ESNode) {
    let mut graph = ESGraph::new();
    let npc = ESNode::new("world", "npc", "testguard");
    let stats = ESNode::new("stats/testguard", "stats", "block")
        .with_prop("passive_perception", ESValue::Number(passive));
    graph.insert(stats);
    (graph, npc)
}

#[test]
fn bold_personality_does_not_secretly_age_the_npc() {
    let npc = ESNode::new("world", "npc", "testnpc")
        .with_prop("personality", ESValue::Text("bold".to_string()));

    let stats = generate_stats(&npc);
    dbg!(&stats);

    assert_eq!(stats.dexterity, 10);

}

