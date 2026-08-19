use super::*;
use super::npc::scope_npc_patch;
use super::player::{build_context, dedupe_by_context, validate_new_character};
use crate::graph::{ESGraph, ESNode, ESValue};
use crate::signal::EventSignal;

// ── merge_patch: namespace authorisation ──────────────────────

#[test]
fn test_sibling_namespace_is_rejected() {
    let mut world = ESGraph::new();
    let mut patch = ESGraph::new();
    patch.insert(
        ESNode::new("inventory/andrew2", "item", "stolen")
            .with_prop("name", ESValue::Text("Stolen".to_string())),
    );

    merge_patch(&mut world, patch, &["world", "inventory/andrew"]);

    assert!(
        world.nodes.get("inventory/andrew2/item:stolen").is_none(),
        "andrew must not be able to write into andrew2's namespace"
    );
}

#[test]
fn test_own_namespace_is_allowed() {
    let mut world = ESGraph::new();
    let mut patch = ESGraph::new();
    patch.insert(ESNode::new("inventory/andrew", "item", "sword"));

    merge_patch(&mut world, patch, &["world", "inventory/andrew"]);

    assert!(world.nodes.get("inventory/andrew/item:sword").is_some());
}

#[test]
fn test_world_prefix_does_not_authorise_lookalike() {
    let mut world = ESGraph::new();
    let mut patch = ESGraph::new();
    patch.insert(ESNode::new("world_secret", "plot", "twist"));

    merge_patch(&mut world, patch, &["world"]);

    assert!(world.nodes.get("world_secret/plot:twist").is_none());
}

// ── NPC patch scoping ─────────────────────────────────────────

fn a_world() -> ESGraph {
    let mut w = ESGraph::new();
    w.insert(ESNode::new("world", "npc", "john_smith"));
    w.insert(ESNode::new("world", "npc", "jin_lyons"));
    w.insert(ESNode::new("world", "player", "andrew"));
    w.insert(ESNode::new("world", "location", "market_district"));
    w.insert(ESNode::new("world", "item", "dropped_coin"));
    w
}

#[test]
fn test_npc_cannot_author_other_characters() {
    let world = a_world();
    let mut patch = ESGraph::new();
    patch.insert(
        ESNode::new("world", "npc", "john_smith")
            .with_prop("alert_level", ESValue::Text("high".to_string())),
    );
    patch.insert(
        ESNode::new("world", "npc", "jin_lyons")
            .with_prop("narrative", ESValue::Text("Jin decides to flee.".to_string())),
    );
    patch.insert(
        ESNode::new("world", "player", "andrew")
            .with_prop("narrative", ESValue::Text("Andrew panics.".to_string())),
    );

    scope_npc_patch(&mut patch, "npc:john_smith", &world);

    assert!(patch.nodes.contains_key("npc:john_smith"), "must keep its own node");
    assert!(!patch.nodes.contains_key("npc:jin_lyons"), "must not write another NPC");
    assert!(!patch.nodes.contains_key("player:andrew"), "must not write the player");
}

#[test]
fn test_npc_may_update_existing_world_objects() {
    let world = a_world();
    let mut patch = ESGraph::new();
    patch.insert(ESNode::new("world", "npc", "john_smith"));
    patch.insert(ESNode::new("world", "location", "market_district"));
    patch.insert(ESNode::new("world", "item", "dropped_coin"));

    scope_npc_patch(&mut patch, "npc:john_smith", &world);

    assert!(patch.nodes.contains_key("location:market_district"));
    assert!(patch.nodes.contains_key("item:dropped_coin"));
}

#[test]
fn test_npc_cannot_invent_places() {
    let world = a_world();
    let mut patch = ESGraph::new();
    patch.insert(ESNode::new("world", "npc", "john_smith"));
    // the real place is `market_district`; this is a phantom beside it
    patch.insert(ESNode::new("world", "location", "market"));

    scope_npc_patch(&mut patch, "npc:john_smith", &world);

    assert!(
        !patch.nodes.contains_key("location:market"),
        "a reacting NPC must not conjure places that do not exist"
    );
}

// ── New characters ────────────────────────────────────────────

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

// ── Cascade dedupe ────────────────────────────────────────────

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

// ── Context building ──────────────────────────────────────────

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
