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

/// Your `bold` test, carried over to the design that replaced the matcher.
///
/// The keyword cascade read `personality: "bold"`, matched it against "old",
/// and quietly docked 2 dexterity — and read `weaknesses: "afraid of soldiers"`
/// as the soldier branch and handed out +4 strength. Stats now derive only from
/// grades, so descriptive text cannot reach them by any path. That is the
/// property worth pinning, and it covers both old bugs at once.
#[test]
fn descriptive_text_cannot_reach_the_stat_block() {
    let plain = ESNode::new("world", "npc", "testnpc")
        .with_prop("physique", ESValue::Text("average".to_string()))
        .with_prop("agility", ESValue::Text("average".to_string()))
        .with_prop("awareness", ESValue::Text("average".to_string()))
        .with_prop("presence", ESValue::Text("average".to_string()));

    let described = plain
        .clone()
        .with_prop("personality", ESValue::Text("bold".to_string()))
        .with_prop("weaknesses", ESValue::Text("afraid of soldiers".to_string()))
        .with_prop("build", ESValue::Text("thin".to_string()));

    let (bare_grades, bare_prof, _) = read_grades(&plain);
    let (desc_grades, desc_prof, _) = read_grades(&described);

    assert_eq!(
        stat_block_from(&bare_grades, &bare_prof),
        stat_block_from(&desc_grades, &desc_prof),
        "narrative props must not alter stats — only grades may"
    );
}

#[test]
fn grades_are_stripped_from_a_patch() {
    let mut node = ESNode::new("world", "npc", "john_smith")
        .with_prop("alert_level", ESValue::Text("high".to_string()))
        .with_prop("awareness", ESValue::Text("exceptional".to_string()))
        .with_prop("proficient", ESValue::Text("perception".to_string()));

    assert!(strip_grade_fields(&mut node), "should report that it removed something");

    assert!(!node.props.contains_key("awareness"), "an NPC cannot regrade itself");
    assert!(!node.props.contains_key("proficient"), "nor re-proficiency itself");
    assert!(node.props.contains_key("alert_level"), "state is still writable");
}

#[test]
fn stripping_a_clean_patch_reports_nothing() {
    let mut node = ESNode::new("world", "npc", "john_smith")
        .with_prop("narrative", ESValue::Text("Watches the crowd.".to_string()));

    assert!(!strip_grade_fields(&mut node));
    assert!(node.props.contains_key("narrative"));
}

