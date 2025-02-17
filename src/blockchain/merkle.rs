use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct MerkleNode {
    left: Option<Box<MerkleNode>>,
    right: Option<Box<MerkleNode>>,
    pub hash: [u8; 32],
}
impl MerkleNode {
    fn new(
        left: Option<Box<MerkleNode>>,
        right: Option<Box<MerkleNode>>,
        data: Option<&[u8]>,
    ) -> MerkleNode {
        // Data is only provided for leaf nodes, so we don't need to consider the input nodes if data exists
        let data_hash = if let Some(data) = data {
            Sha256::digest(data)
        } else {
            // If no data is present, we are in the middle of the tree.
            // We must calculate the combined hash of the given nodes.
            let mut combined = Vec::new();
            if let Some(ref left) = left {
                combined.extend_from_slice(&left.hash);
            }
            if let Some(ref right) = right {
                combined.extend_from_slice(&right.hash);
            }
            Sha256::digest(&combined)
        };

        // New node stores child nodes and the calculated data hash.
        MerkleNode {
            left,
            right,
            hash: data_hash.into(),
        }
    }
}

#[derive(Debug)]
/// A Merkle tree is a crucial data structure for blockchains. A tree allows us to
/// check the validity of a tx without having to check every transaction, or even check the blocks themselves.
/// A tree of hashes is built, where the leaf nodes are hashes of data (transactions), and the parent nodes are hashes
/// of the concatenation of the child hashes. The root node therefore represents all data in the blockchain.
pub struct MerkleTree {
    pub root: Box<MerkleNode>,
}

impl MerkleTree {
    pub fn new(data: Vec<Vec<u8>>) -> MerkleTree {
        // Each tx will represent a leaf node. We must first gather all leaf nodes
        // to construct the tree from the bottom up.
        let mut nodes: Vec<Box<MerkleNode>> = data
            .into_iter()
            // Map a merkle node. Nodes will have no L/R, only a data hash
            .map(|d| Box::new(MerkleNode::new(None, None, Some(&d))))
            .collect();
        if nodes.is_empty() {
            panic!("[MerkleTree::new] ERROR: No Merkle nodes")
        }

        // Run until we only have the root node left
        while nodes.len() > 1 {
            // If there is an odd number of nodes, duplicate the last node to make it even
            // Every node must have a pair to compute parent nodes
            if nodes.len() % 2 != 0 {
                let last_node = nodes.last().unwrap().clone();
                nodes.push(last_node);
            }

            // Create a new level to store the parent nodes
            let mut new_level = Vec::new();

            // Loop through nodes by a step of 2, creating a new parent by merging
            // The current 2 leaf nodes into 1 parent node. Data is set to None to trigger
            // the use of the child nodes to create a new hash. The new hash is stored
            // in the hash field for use when the parent node becomes a child in the next loop.
            for i in (0..nodes.len()).step_by(2) {
                let parent = Box::new(MerkleNode::new(
                    Some(nodes[i].clone()),
                    Some(nodes[i + 1].clone()),
                    None,
                ));
                new_level.push(parent);
            }
            // Reset nodes to the new level of parent nodes before restarting the loop
            nodes = new_level;
        }

        // Loop stops after constructing the root node, since there would only be 1 parent created for the level.
        MerkleTree {
            root: nodes.remove(0),
        }
    }
}
