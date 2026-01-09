use std::collections::{BTreeSet, HashMap};

/// A node that can participate in a dependency order.
/// - `key()` must be stable and unique within the batch.
/// - `deps()` may include external deps; they’ll be ignored automatically.
pub trait TopoNode {
    fn key(&self) -> &str;
    fn deps(&self) -> impl Iterator<Item = &str>;
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TopoError {
    #[error("duplicate node key: {0}")]
    DuplicateKey(String),

    #[error("cycle detected; remaining nodes (sample): {0:?}")]
    Cycle(Vec<String>),
}

/// Returns indices into `nodes` in a safe creation order.
/// 
/// **Determinism**: Output order is stable across runs. When multiple nodes
/// have zero in-degree, they are processed lexicographically by `key()`.
/// This ensures reproducible migrations regardless of hash map iteration order.
pub fn toposort_indices<N: TopoNode>(nodes: &[N]) -> Result<Vec<usize>, TopoError> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    // key -> index
    let mut idx_of: HashMap<String, usize> = HashMap::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        let k = n.key().to_string();
        if idx_of.insert(k.clone(), i).is_some() {
            return Err(TopoError::DuplicateKey(k));
        }
    }

    // adjacency: dep_index -> Vec<dependent_index>
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut indeg: Vec<usize> = vec![0; nodes.len()];

    for (i, n) in nodes.iter().enumerate() {
        // Count only deps that exist in batch; de-dupe to avoid double edges
        let mut dep_indices: Vec<usize> = n.deps()
            .filter_map(|d| idx_of.get(d).copied())
            .collect();
        
        if !dep_indices.is_empty() {
            dep_indices.sort_unstable();
            dep_indices.dedup();
            
            for j in dep_indices {
                out[j].push(i);
                indeg[i] += 1;
            }
        }
    }

    // queue of zero indegree nodes, deterministic order by key
    let mut zero: BTreeSet<String> = BTreeSet::new();
    for (k, &i) in idx_of.iter() {
        if indeg[i] == 0 {
            zero.insert(k.clone());
        }
    }

    let mut order = Vec::with_capacity(nodes.len());
    while let Some(k) = zero.pop_first() {
        let i = idx_of[&k];
        order.push(i);

        // deterministically release dependents by sorting their keys
        let mut newly_zero: Vec<String> = Vec::new();
        for &dep_i in &out[i] {
            indeg[dep_i] -= 1;
            if indeg[dep_i] == 0 {
                newly_zero.push(nodes[dep_i].key().to_string());
            }
        }
        newly_zero.sort();
        for nk in newly_zero {
            zero.insert(nk);
        }
    }

    if order.len() != nodes.len() {
        // sample a few remaining nodes for debugging
        let mut rem: Vec<String> = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (indeg[i] > 0).then(|| n.key().to_string()))
            .collect();
        rem.sort();
        rem.truncate(8);
        return Err(TopoError::Cycle(rem));
    }

    Ok(order)
}

/// Convenience: return borrowed nodes in topo order.
pub fn toposort_refs<'a, N: TopoNode>(nodes: &'a [N]) -> Result<Vec<&'a N>, TopoError> {
    let idx = toposort_indices(nodes)?;
    Ok(idx.into_iter().map(|i| &nodes[i]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Node {
        name: String,
        deps: Vec<String>,
    }

    impl TopoNode for Node {
        fn key(&self) -> &str {
            &self.name
        }

        fn deps(&self) -> impl Iterator<Item = &str> {
            self.deps.iter().map(|s| s.as_str())
        }
    }

    fn node(name: &str, deps: &[&str]) -> Node {
        Node {
            name: name.to_string(),
            deps: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_empty() {
        let nodes: Vec<Node> = vec![];
        let result = toposort_indices(&nodes);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn test_single_no_deps() {
        let nodes = vec![node("a", &[])];
        let result = toposort_indices(&nodes).unwrap();
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_two_independent() {
        let nodes = vec![node("b", &[]), node("a", &[])];
        let result = toposort_indices(&nodes).unwrap();
        // Should be lexicographic: a before b
        assert_eq!(result, vec![1, 0]);
    }

    #[test]
    fn test_linear_chain() {
        let nodes = vec![
            node("c", &["b"]),
            node("b", &["a"]),
            node("a", &[]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // Must be a -> b -> c
        assert_eq!(result, vec![2, 1, 0]);
    }

    #[test]
    fn test_diamond_pattern() {
        let nodes = vec![
            node("d", &["b", "c"]),
            node("b", &["a"]),
            node("c", &["a"]),
            node("a", &[]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // a first, then b and c (lexicographically), then d
        assert_eq!(result, vec![3, 1, 2, 0]);
    }

    #[test]
    fn test_cycle_two_nodes() {
        let nodes = vec![node("a", &["b"]), node("b", &["a"])];
        let result = toposort_indices(&nodes);
        assert!(result.is_err());
        match result {
            Err(TopoError::Cycle(remaining)) => {
                assert_eq!(remaining.len(), 2);
                assert!(remaining.contains(&"a".to_string()));
                assert!(remaining.contains(&"b".to_string()));
            }
            _ => panic!("Expected Cycle error"),
        }
    }

    #[test]
    fn test_cycle_three_nodes() {
        let nodes = vec![
            node("a", &["b"]),
            node("b", &["c"]),
            node("c", &["a"]),
        ];
        let result = toposort_indices(&nodes);
        assert!(result.is_err());
        match result {
            Err(TopoError::Cycle(remaining)) => {
                assert_eq!(remaining.len(), 3);
            }
            _ => panic!("Expected Cycle error"),
        }
    }

    #[test]
    fn test_self_reference_cycle() {
        let nodes = vec![node("a", &["a"])];
        let result = toposort_indices(&nodes);
        assert!(result.is_err());
        match result {
            Err(TopoError::Cycle(remaining)) => {
                assert_eq!(remaining, vec!["a".to_string()]);
            }
            _ => panic!("Expected Cycle error"),
        }
    }

    #[test]
    fn test_external_deps_ignored() {
        let nodes = vec![
            node("b", &["a"]),
            node("a", &["external", "other"]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // 'external' and 'other' not in batch, so 'a' has in-degree 0
        assert_eq!(result, vec![1, 0]);
    }

    #[test]
    fn test_duplicate_deps() {
        let nodes = vec![
            node("b", &["a", "a", "a"]),
            node("a", &[]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // Duplicates should be handled correctly (deduped)
        assert_eq!(result, vec![1, 0]);
    }

    #[test]
    fn test_duplicate_key_error() {
        let nodes = vec![node("a", &[]), node("a", &["b"])];
        let result = toposort_indices(&nodes);
        assert!(result.is_err());
        match result {
            Err(TopoError::DuplicateKey(key)) => {
                assert_eq!(key, "a");
            }
            _ => panic!("Expected DuplicateKey error"),
        }
    }

    #[test]
    fn test_deterministic_ordering() {
        // Multiple zero-degree nodes should be processed lexicographically
        let nodes = vec![
            node("z", &[]),
            node("a", &[]),
            node("m", &[]),
            node("b", &[]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // Should be alphabetical: a, b, m, z
        assert_eq!(result, vec![1, 3, 2, 0]);
    }

    #[test]
    fn test_deterministic_with_deps() {
        // When multiple dependents become available, process lexicographically
        let nodes = vec![
            node("root", &[]),
            node("child_z", &["root"]),
            node("child_a", &["root"]),
            node("child_m", &["root"]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // root, then children alphabetically
        assert_eq!(result, vec![0, 2, 3, 1]);
    }

    #[test]
    fn test_complex_graph() {
        let nodes = vec![
            node("f", &["d", "e"]),
            node("e", &["b", "c"]),
            node("d", &["b", "c"]),
            node("c", &["a"]),
            node("b", &["a"]),
            node("a", &[]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // a first, then b and c, then d and e, then f
        let order: Vec<String> = result.iter().map(|&i| nodes[i].name.clone()).collect();
        
        // Verify ordering constraints
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
        assert!(pos("b") < pos("e"));
        assert!(pos("c") < pos("e"));
        assert!(pos("d") < pos("f"));
        assert!(pos("e") < pos("f"));
    }

    #[test]
    fn test_toposort_refs() {
        let nodes = vec![
            node("c", &["b"]),
            node("b", &["a"]),
            node("a", &[]),
        ];
        let result = toposort_refs(&nodes).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "a");
        assert_eq!(result[1].name, "b");
        assert_eq!(result[2].name, "c");
    }

    #[test]
    fn test_mixed_internal_external_deps() {
        let nodes = vec![
            node("c", &["b", "external1"]),
            node("b", &["a", "external2"]),
            node("a", &["external3"]),
        ];
        let result = toposort_indices(&nodes).unwrap();
        // All external deps ignored, should still get a -> b -> c
        assert_eq!(result, vec![2, 1, 0]);
    }

    #[test]
    fn test_partial_cycle() {
        // Some nodes form a cycle, others don't
        let nodes = vec![
            node("d", &["c"]),
            node("c", &[]),
            node("b", &["a"]),
            node("a", &["b"]),
        ];
        let result = toposort_indices(&nodes);
        assert!(result.is_err());
        match result {
            Err(TopoError::Cycle(remaining)) => {
                // Should only contain nodes in cycle
                assert_eq!(remaining.len(), 2);
                assert!(remaining.contains(&"a".to_string()));
                assert!(remaining.contains(&"b".to_string()));
            }
            _ => panic!("Expected Cycle error"),
        }
    }
}