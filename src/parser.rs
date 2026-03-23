use crate::graph::{ESGraph, ESNode, ESValue};

enum LineType {
    Comment,
    InlineEdge,
    NodeDecl,
    Edge,
    Property,
    Empty,
}

fn classify(line: &str) -> LineType {
    if line.starts_with('#') { LineType::Comment }
    else if line.starts_with('@') && line.contains("--[") { LineType::InlineEdge }
    else if line.starts_with('@') { LineType::NodeDecl }
    else if line.starts_with("--[") { LineType::Edge }
    else if line.contains(": ") { LineType::Property }
    else { LineType::Empty }
}

pub fn parse(input: &str) -> ESGraph {
    let mut graph = ESGraph::new();
    let mut current: Option<ESNode> = None;
    
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        match classify(line) {
            LineType::Comment    => { continue; }
            LineType::NodeDecl   => {
                if let Some(node) = current {
                    graph.insert(node);
                }
                let line = line.trim_start_matches("@");
                let parts: Vec<&str> = line.splitn(2, ":").collect();
                let n_type = parts[0];
                let n_id = parts[1];

                current = Some(ESNode::new(n_type, n_id));
            }
            LineType::Property   => {
                if let Some(node) = current.as_mut() {
                    
                    let parts: Vec<&str> = line.splitn(2, ": ").collect();
                    let key = parts[0].trim();
                    let raw = parts[1].trim();

                    let value = if raw == "true" {
                        ESValue::Bool(true)
                    } else if raw == "false" {
                        ESValue::Bool(false)
                    } else if let Ok(n) = raw.parse::<f64>() {
                        ESValue::Number(n)
                    } else {
                        ESValue::Text(raw.trim_matches('"').to_string())
                    };

                    node.props.insert(key.to_string(), value);
                }
            }
            LineType::Edge       => {}
            LineType::InlineEdge => {}
            LineType::Empty      => {}
        }
    }

    if let Some(node) = current {
        graph.insert(node);
    }

    graph
}