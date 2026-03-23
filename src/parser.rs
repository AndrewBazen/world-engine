use crate::graph::{ESGraph, ESNode, ESValue, ESEdge};

enum LineType {
    Comment,
    InlineEdge,
    NodeDecl,
    Edge,
    Property,
    Empty
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
            LineType::Empty      => { continue; }
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
            LineType::Edge       => {
                if let Some(node) = current.as_mut() {
                    let line = line.strip_prefix("--[").unwrap_or(line);

                    let parts: Vec<&str> = line.splitn(2, "]-->").collect();
                    let label = parts[0];

                    let mut target = parts[1].trim();
                    target = target.trim_start_matches("@");
                    let target_parts: Vec<&str> = target.splitn(2, ":").collect();
                    let target_type = target_parts[0];
                    let target_id = target_parts[1];

                    node.edges.push(ESEdge::new(label, target_type, target_id));
                }
            }
            LineType::InlineEdge => {
                // split the inline edge
                let inline_parts: Vec<&str> = line.splitn(2, " --[").collect();

                // parse the node declaration
                if let Some(node) = current {
                    graph.insert(node);
                }
                let new_node = inline_parts[0].trim_start_matches("@");
                let n_parts: Vec<&str> = new_node.splitn(2, ":").collect();
                let n_type = n_parts[0];
                let n_id = n_parts[1];

                current = Some(ESNode::new(n_type, n_id));

                // parse the edge
                if let Some(node) = current.as_mut() {
                    let parts: Vec<&str> = inline_parts[1].splitn(2, "]-->").collect();
                    let label = parts[0];

                    let mut target = parts[1].trim();
                    target = target.trim_start_matches("@");
                    let target_parts: Vec<&str> = target.splitn(2, ":").collect();
                    let target_type = target_parts[0];
                    let target_id = target_parts[1];

                    node.edges.push(ESEdge::new(label, target_type, target_id));
                }

                // flush current
                if let Some(node) = current.take() {
                    graph.insert(node);
                }
            }
        }
    }

    if let Some(node) = current {
        graph.insert(node);
    }

    graph
}