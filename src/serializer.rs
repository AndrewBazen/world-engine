use crate::graph::{ESGraph, ESNode, ESValue};

pub fn serialize(graph: &ESGraph) -> String {
    let mut output = String::new();

    for node in graph.nodes.values() {
        output.push_str(&format!("@{}:{}\n", node.node_type, node.id));
        add_props(&node, &mut output);
        add_edges(&node, &mut output);
        output.push('\n');
    }

    output
}

fn add_props(node: &ESNode, output: &mut String) {
    for (key, value) in node.props.iter() {
        match value {
            ESValue::Text(s) => output.push_str(&format!("  {}: \"{}\"\n", key, s)),
            ESValue::Number(n) => output.push_str(&format!("  {}: {}\n", key, n)),
            ESValue::Bool(b) => output.push_str(&format!("  {}: {}\n", key, b)),
        }
    }
}

fn add_edges(node: &ESNode, output: &mut String) {
    for edge in node.edges.iter() {
        output.push_str(&format!("  --[{}]--> @{}:{}\n", edge.label, edge.target_type, edge.target_id));
    }
}