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

/// Split a node declaration into (namespace, "type:id").
/// `inventory/andrew/item:sword` → ("inventory/andrew", "item:sword")
/// `player:andrew`               → ("world", "player:andrew")
fn split_namespace(decl: &str) -> (&str, &str) {
    match decl.rfind('/') {
        Some(i) => (&decl[..i], &decl[i + 1..]),
        None => ("world", decl),
    }
}

/// Split an edge line into (label, raw target). Tolerates the arrow being
/// written with one or two dashes, which models get wrong regularly.
/// Returns None rather than panicking on anything else.
fn split_edge(line: &str) -> Option<(&str, &str)> {
    let body = line.trim_start().strip_prefix("--[")?;
    for arrow in ["]-->", "]->", "]-"] {
        if let Some(pair) = body.split_once(arrow) {
            return Some(pair);
        }
    }
    None
}

/// Parse an edge target into (type, id), discarding any namespace prefix so
/// `@inventory/andrew/item:sword` yields ("item", "sword") rather than a
/// target_type containing the whole path.
fn parse_edge_target(raw: &str) -> Option<(&str, &str)> {
    let t = raw.trim().trim_start_matches('@');
    let t = match t.rfind('/') {
        Some(i) => &t[i + 1..],
        None => t,
    };
    let (ty, id) = t.split_once(':')?;
    let (ty, id) = (ty.trim(), id.trim());
    if ty.is_empty() || id.is_empty() {
        return None;
    }
    Some((ty, id))
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
            LineType::NodeDecl => {
                if let Some(node) = current.take() {
                    graph.merge_node(node);
                }
            
                let line = line.trim_start_matches('@');

                let (namespace, type_and_id) = split_namespace(line);

                let Some((n_type, n_id)) = type_and_id.split_once(':') else {
                    eprintln!("skipping malformed node declaration: {}", line);
                    continue;
                };

                current = Some(ESNode::new(namespace, n_type.trim(), n_id.trim()));
            }
            LineType::Property   => {
                if let Some(node) = current.as_mut() {
                    
                    let parts: Vec<&str> = line.splitn(2, ": ").collect();
                    if parts.len() < 2 {
                        eprintln!("skipping malformed property declaration: {}", line);
                        continue;
                    }
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
                let Some(node) = current.as_mut() else {
                    eprintln!("skipping edge with no node declared above it: {}", line);
                    continue;
                };

                let Some((label, target_raw)) = split_edge(line) else {
                    eprintln!("skipping malformed edge: {}", line);
                    continue;
                };

                match parse_edge_target(target_raw) {
                    Some((target_type, target_id)) => {
                        node.edges.push(ESEdge::new(label.trim(), target_type, target_id));
                    }
                    None => {
                        eprintln!("skipping malformed edge target: {}", target_raw.trim());
                    }
                }
            }
            LineType::InlineEdge => {
                // "@node:decl --[label]--> @target:id" — split at the arrow.
                // Note: no leading space required, models often omit it.
                let Some(arrow_at) = line.find("--[") else {
                    eprintln!("skipping malformed inline edge: {}", line);
                    continue;
                };
                let (decl_part, edge_part) = line.split_at(arrow_at);

                // flush whatever node we were building
                if let Some(node) = current.take() {
                    graph.merge_node(node);
                }

                let decl = decl_part.trim().trim_start_matches('@');
                let (namespace, type_and_id) = split_namespace(decl);

                let Some((n_type, n_id)) = type_and_id.split_once(':') else {
                    eprintln!("skipping malformed inline node declaration: {}", decl);
                    continue;
                };

                let mut node = ESNode::new(namespace, n_type.trim(), n_id.trim());

                match split_edge(edge_part).and_then(|(label, target_raw)| {
                    parse_edge_target(target_raw).map(|(ty, id)| (label, ty, id))
                }) {
                    Some((label, target_type, target_id)) => {
                        node.edges.push(ESEdge::new(label.trim(), target_type, target_id));
                    }
                    None => {
                        eprintln!("skipping malformed inline edge target: {}", edge_part.trim());
                    }
                }

                graph.merge_node(node);
            }
        }
    }

    if let Some(node) = current.take() {
        graph.merge_node(node);
    }

    graph
}
