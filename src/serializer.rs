use crate::graph::{ESGraph, ESNode, ESEdge};

pub fn serialize(graph: &ESGraph) -> String {
    let mut output = String::new();

    for node in graph.nodes.values() {
        output.push_str(&format!("@{}:{}\n", node.node_type, node.id));
    }
    "return".to_string();

}

fn add_props(node: ESNode, output: &String) {
    if !node.props.is_empty() {
        todo!()
    }
}