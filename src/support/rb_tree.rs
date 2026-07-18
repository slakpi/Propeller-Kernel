//! Red-Black Tree Utilities
//!
//!   NOTE: The tree is backed by a vector and should NOT be used to store large
//!         objects.
//!
//! See https://en.wikipedia.org/wiki/Red%E2%80%93black_tree

#[cfg(feature = "module_tests")]
mod tests;

use super::vector::Vector;
use crate::debug_print;
use crate::support::memory::PageAllocator;
use crate::sync::SpinLock;
use core::cmp::Ordering;
use core::iter::IntoIterator;
use core::mem;

const LEFT: usize = 0;
const RIGHT: usize = 1;

/// Tree node color.
#[derive(Clone, Copy, PartialEq)]
enum Color {
  RED,
  BLACK,
}

/// Tree node.
struct Node<K, V>
where
  K: Sized + PartialOrd,
{
  key: K,
  value: V,
  parent: usize,
  children: [usize; 2],
  color: Color,
}

/// Implements a red-black tree for a specified type. To simplify allocation and
/// relationships, the tree is stored in a linear vector and indices are used
/// rather than pointers.
pub struct RedBlackTree<'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  nodes: Vector<'alloc, A, Option<Node<K, V>>>,
  free: Vector<'alloc, A, usize>,
  root: usize,
}

impl<'alloc, A, K, V> RedBlackTree<'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  /// Construct a new red-black tree.
  ///
  /// # Parameters
  ///
  /// * `allocator` - A page allocator for the internal vectors.
  pub fn new(allocator: &'alloc SpinLock<A>) -> Self {
    Self {
      nodes: Vector::<'alloc, A, Option<Node<K, V>>>::new(allocator),
      free: Vector::<'alloc, A, usize>::new(allocator),
      root: 0,
    }
  }

  /// Determine if the tree is empty.
  pub fn is_empty(&self) -> bool {
    self.root == 0
  }

  /// Get a reference to the value stored at the specified index.
  ///
  /// # Parameters
  ///
  /// * `index` - The index obtained from a key search.
  ///
  /// # Returns
  ///
  /// A reference to the value, or None if the index is invalid.
  pub fn get_value(&self, index: usize) -> Option<&'_ V> {
    if index == 0 || index >= self.nodes.len() {
      return None;
    }

    Some(&self.nodes[index].as_ref()?.value)
  }

  /// Get a mutable reference to the value stored at the specified index.
  ///
  /// # Parameters
  ///
  /// * `index` - The index obtained from a key search.
  ///
  /// # Returns
  ///
  /// A mutable reference to the value, or None if the index is invalid.
  pub fn get_value_mut(&mut self, index: usize) -> Option<&'_ mut V> {
    if index == 0 || index >= self.nodes.len() {
      return None;
    }

    Some(&mut self.nodes[index].as_mut()?.value)
  }

  /// Find the first instance of the specified key.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to search for.
  ///
  /// # Returns
  ///
  /// The index of the first instance of the specified key, or  None if one is
  /// not found.
  pub fn find(&self, key: K) -> Option<usize> {
    let mut current_index = self.root;

    while current_index != 0 {
      let node = self.nodes[current_index].as_ref().unwrap();

      if node.key < key {
        current_index = node.children[RIGHT];
      } else if node.key == key {
        return Some(current_index);
      } else {
        current_index = node.children[LEFT];
      }
    }

    None
  }

  /// Find the first key not less than the specified key.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to search for.
  ///
  /// # Returns
  ///
  /// The index of the first item with a key not less than the specified key, or
  /// None if one is not found.
  pub fn lower_bound(&self, key: K) -> Option<usize> {
    let mut last_index = 0;
    let mut current_index = self.root;

    while current_index != 0 {
      let node = self.nodes[current_index].as_ref().unwrap();

      if node.key < key {
        current_index = node.children[RIGHT];
      } else if node.key == key {
        return Some(current_index);
      } else {
        last_index = current_index;
        current_index = node.children[LEFT];
      }
    }

    if last_index == 0 {
      None
    } else {
      Some(last_index)
    }
  }

  /// Find the first key greater than the specified key.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to search for.
  ///
  /// # Returns
  ///
  /// The index of the first item with a key greater than the specified key, or
  /// None if one is not found.
  pub fn upper_bound(&self, key: K) -> Option<usize> {
    let mut last_index = 0;
    let mut current_index = self.root;

    while current_index != 0 {
      let node = self.nodes[current_index].as_ref().unwrap();

      if node.key <= key {
        current_index = node.children[RIGHT];
      } else {
        last_index = current_index;
        current_index = node.children[LEFT];
      }
    }

    if last_index == 0 {
      None
    } else {
      Some(last_index)
    }
  }

  /// Get an in-order traversal iterator.
  pub fn iter(&self) -> InOrderIterator<'_, 'alloc, A, K, V> {
    InOrderIterator::new(self)
  }

  /// Get a mutable in-order traversal iterator.
  pub fn iter_mut(&mut self) -> InOrderIteratorMut<'_, 'alloc, A, K, V> {
    InOrderIteratorMut::new(self)
  }

  /// Insert an object into the tree.
  ///
  /// # Parameters
  ///
  /// * `key` - The object key.
  /// * `value` - The object to insert.
  ///
  /// # Returns
  ///
  /// The index of the new node or None if unable to insert the node.
  pub fn insert(&mut self, key: K, value: V) -> Option<usize> {
    let index = self.binary_tree_insert(key, value)?;
    self.balance(index);
    Some(index)
  }

  /// Erase the first instance of an object with the specified key.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to erase.
  ///
  /// # Description
  ///
  /// Erasure invalidates object indices and iterators.
  ///
  /// # Returns
  ///
  /// A tuple with the key and object erased, or None if the key was not found.
  pub fn erase(&mut self, key: K) -> Option<(K, V)> {
    let index = self.find(key)?;
    self.remove(index)
  }

  /// Remove an object from the tree.
  ///
  /// # Parameters
  ///
  /// * `index` - The index of the object to remove.
  ///
  /// # Description
  ///
  /// Removal invalidates object indices and iterators.
  ///
  /// # Returns
  ///
  /// A tuple with the key and object erased, or None if the key was not found.
  pub fn remove(&mut self, index: usize) -> Option<(K, V)> {
    // If the node has no children, it can be directly removed from the tree by
    // setting the appropriate child pointer on the parent to None.
    //
    //           Delete A
    //     P                  P
    //      \
    //      (A)
    //
    // If the node has one child, the node's child simply replaces it.
    //
    //           Delete A
    //     P                  P
    //      \                  \
    //      (A)                 C
    //      /
    //     C
    //
    // If the node has two children, its successor replaces it. Since the node
    // has two children, it is guaranteed to have a successor. The trick here,
    // however, is that the successor node is the one that is actually removed.
    //
    //           Delete A
    //     P                  P
    //      \                  \
    //      (A)                 D
    //      / \                / \
    //     C   B              C   B
    //        / \                / \
    //      (D)  E              F   E
    //        \
    //         F
    //
    // After removal, the tree is traversed and updated to restore the red-black
    // properties.

    // Index must be within the bounds of the vector and must not be the
    // sentinel object.
    if index == 0 || index >= self.nodes.len() {
      return None;
    }

    // Deletion must have overlapping mutable references to nodes in the tree.
    // The modifications are well-defined, and we do not need the borrow checker
    // to protect us here. Modifications will go through raw pointers.
    let vec_ptr = self.nodes.as_mut_ptr();

    let node = Self::get_node_unchecked_mut(vec_ptr, index);

    // Find the actual node to delete from the tree. In the zero- and one-child
    // cases, this will be the specified node. In the two-child case, however,
    // the actual node deleted will be the successor node.
    let del_index = if node.children[LEFT] == 0 || node.children[RIGHT] == 0 {
      index
    } else {
      self.binary_tree_successor(index).unwrap()
    };

    // Attempt to add the index of the actual node being deleted to the free
    // list. Up to this point, the tree has not been modified. If adding the
    // index to the free list fails, we have to fail to removal process.
    if !self.free.push(del_index) {
      return None;
    }

    let del = Self::get_node_unchecked_mut(vec_ptr, del_index);

    // By definition, the actual node being deleted can only have at most one
    // child. If it does have a child, make its parent the child's parent. Refer
    // to the diagrams above. For the one-child case, this makes C's parent P.
    // For the two-child case, it makes F's parent B. If the actual node being
    // deleted does not have a child, use the sentinel object.
    let child_index = if del.children[LEFT] != 0 {
      del.children[LEFT]
    } else {
      del.children[RIGHT]
    };
    let child = Self::get_node_unchecked_mut(vec_ptr, child_index);
    child.parent = del.parent;

    // If the actual node being deleted has a parent, finish removing the node
    // from the tree by updating the parent's child pointer. Otherwise, make the
    // tree's root the child.
    if del.parent != 0 {
      let parent = Self::get_node_unchecked_mut(vec_ptr, del.parent);
      let direction = Self::get_node_direction(del_index, parent);
      parent.children[direction] = child_index;
    } else {
      self.root = child_index;
    }

    // In the two-child case, the actual node being deleted will be different
    // from the specified node. Refer to the two-child case diagram above.
    // Swapping the objects effectively moves D into A's place as shown.
    if del_index != index {
      mem::swap(&mut node.key, &mut del.key);
      mem::swap(&mut node.value, &mut del.value);
    }

    // If the actual node being deleted is black, run the fixup starting from
    // the child of the actual node deleted. This could be the sentinel object.
    if del.color == Color::BLACK {
      self.fixup_after_removal(child_index);
    }

    // In all cases, the actual node being deleted has been fully removed from
    // the tree and has the object to return.
    let del = self.nodes[del_index].take().unwrap();

    // If the tree is empty, free all of its resources.
    if self.is_empty() {
      self.nodes.truncate(0);
      self.free.truncate(0);
    }

    Some((del.key, del.value))
  }

  /// Empty the tree.
  pub fn empty(&mut self) {
    self.nodes.truncate(0);
    self.free.truncate(0);
    self.root = 0;
  }

  /// Binary tree insertion.
  ///
  /// # Parameters
  ///
  /// * `key` - The object key.
  /// * `value` - The object to insert.
  ///
  /// # Returns
  ///
  /// The index of the new object, or None if it could not be inserted.
  fn binary_tree_insert(&mut self, key: K, value: V) -> Option<usize> {
    let index = self.stage_node(key, value)?;

    if let Some((parent_index, ordering)) = self.find_insert_parent(index) {
      let n = self.nodes[parent_index].as_mut().unwrap();

      match ordering {
        Ordering::Less => n.children[LEFT] = index,
        _ => n.children[RIGHT] = index,
      }

      let n = self.nodes[index].as_mut().unwrap();
      n.parent = parent_index;
    } else {
      self.root = index;
    }

    Some(index)
  }

  /// Find a node's successor.
  ///
  /// # Parameters
  ///
  /// * `index` - The node.
  ///
  /// # Assumptions
  ///
  /// Assumes `index` is valid.
  ///
  /// # Returns
  ///
  /// The index of the node's successor, or None if no successor exists.
  fn binary_tree_successor(&self, index: usize) -> Option<usize> {
    let node = self.nodes[index].as_ref().unwrap();

    // If the node has a right child, its successor is just the minimum node on
    // the right subtree.
    if node.children[RIGHT] != 0 {
      return Some(self.binary_tree_minimum(node.children[RIGHT]));
    }

    // Otherwise, work upward through the tree until we find a node that is the
    // left child of its parent. The parent node is the successor.
    let mut current_index = index;
    let mut parent_index = node.parent;

    loop {
      if parent_index == 0 {
        break;
      };

      let parent = self.nodes[parent_index].as_ref().unwrap();

      if current_index == parent.children[LEFT] {
        break;
      }

      current_index = parent_index;
      parent_index = parent.parent;
    }

    Some(parent_index)
  }

  /// Finds the minimum node on a subtree.
  ///
  /// # Parameters
  ///
  /// * `index` - The root of the subtree.
  ///
  /// # Assumptions
  ///
  /// Assumes `index` is valid.
  ///
  /// # Returns
  ///
  /// The index of the minimum node on the subtree.
  fn binary_tree_minimum(&self, index: usize) -> usize {
    let mut min = index;

    loop {
      let node = self.nodes[min].as_ref().unwrap();

      if node.children[LEFT] == 0 {
        break;
      }

      min = node.children[LEFT];
    }

    min
  }

  /// Stages a new node for tree insertion.
  ///
  /// # Parameters
  ///
  /// * `key` - The object key.
  /// * `value` - The object to insert.
  ///
  /// # Returns
  ///
  /// The index of the new node, or None if unable to insert the node.
  fn stage_node(&mut self, key: K, value: V) -> Option<usize> {
    // If the nodes vector has not been allocated yet, push the sentinel object
    // to the vector. The sentinel object resides at index 0 and is never
    // deleted. It is used to simplify the removal process. Its object must
    // never be accessed, and its color must never be changed from black.
    //
    // If we fail to allocate the vector and push the sentinel, we cannot stage
    // a new object.
    if self.nodes.is_empty() {
      let sentinel = Node::<K, V> {
        key: unsafe { mem::zeroed::<K>() },
        value: unsafe { mem::zeroed::<V>() },
        parent: 0,
        children: [0; 2],
        color: Color::BLACK,
      };

      if !self.nodes.push(Some(sentinel)) {
        return None;
      }
    }

    let node = Node::<K, V> {
      key,
      value,
      parent: 0,
      children: [0, 0],
      color: Color::RED,
    };

    // Attempt to reuse an existing node if one is available.
    if let Some(index) = self.free.pop() {
      self.nodes[index] = Some(node);
      return Some(index);
    }

    // Otherwise, attempt to push a new node to the node vector.
    if self.nodes.push(Some(node)) {
      return Some(self.nodes.len() - 1);
    }

    // Failed to stage the node.
    None
  }

  /// Search for the parent of a staged node.
  ///
  /// # Parameters
  ///
  /// * `index` - The index of the staged node.
  ///
  /// # Returns
  ///
  /// A tuple with the index of the parent node and the new node's ordering, or
  /// None if the tree is empty.
  fn find_insert_parent(&self, index: usize) -> Option<(usize, Ordering)> {
    if self.is_empty() {
      return None;
    }

    let mut parent_index;
    let mut current_index = self.root;
    let mut ordering;

    let node = self.nodes[index].as_ref().unwrap();

    loop {
      let current = self.nodes[current_index].as_ref().unwrap();

      parent_index = current_index;

      // An ordering must exist.
      ordering = node.key.partial_cmp(&current.key).unwrap();
      match ordering {
        Ordering::Less => current_index = current.children[LEFT],
        _ => current_index = current.children[RIGHT],
      }

      if current_index == 0 {
        break;
      }
    }

    // The empty case is handled at the beginning, so we know we will have a
    // parent node.
    Some((parent_index, ordering))
  }

  /// Re-balance the tree after a binary tree insert.
  ///
  /// # Parameters
  ///
  /// * `index` - The index of the newly inserted node.
  ///
  /// # Assumptions
  ///
  /// Assumes the node at `index` is valid.
  fn balance(&mut self, index: usize) {
    // Balance must have overlapping mutable references to nodes in the tree.
    // The modifications are well-defined, and we do not need the borrow checker
    // to protect us here. Modifications will go through raw pointers.
    let vec_ptr = self.nodes.as_mut_ptr();

    // Track the current node.
    let mut node_index = index;
    let mut node = Self::get_node_unchecked_mut(vec_ptr, node_index);

    loop {
      // If we reached the root of the tree, we are done.
      if node.parent == 0 {
        break;
      }

      let parent_index = node.parent;
      let mut parent = Self::get_node_unchecked_mut(vec_ptr, parent_index);

      // New nodes are red leaves. If the parent is black, then rule 3 is
      // satisfied.
      if parent.color == Color::BLACK {
        break;
      }

      // If the grandparent is the root, we're done.
      if parent.parent == 0 {
        break;
      };

      let grandparent_index = parent.parent;
      let grandparent = Self::get_node_unchecked_mut(vec_ptr, grandparent_index);

      // Check if the parent has a sibling.
      let direction = Self::get_node_direction(parent_index, grandparent);
      let parent_sibling: Option<&mut Node<K, V>> = if grandparent.children[1 - direction] != 0 {
        Some(Self::get_node_unchecked_mut(vec_ptr, grandparent.children[1 - direction]))
      } else {
        None
      };

      // If the parent has a red sibling, then both can be made black and the
      // grandparent red to satisfy rule 4. However, we will need to continue
      // up the tree.
      if let Some(parent_sibling) = parent_sibling
        && parent_sibling.color == Color::RED
      {
        parent.color = Color::BLACK;
        parent_sibling.color = Color::BLACK;
        grandparent.color = Color::RED;
        node = grandparent;
        node_index = grandparent_index;
        continue;
      }

      // Rotations: Nodes are never moved within the vector, so the references
      // will remain valid after rotations.

      // Ensure the current node is an outer grandchild, e.g. parent is the
      // left child of grandparent and node is the left child of parent, or
      // parent is the right child of grandparent and node is the right child
      // of parent.
      //
      // Example: If parent is the right child of grandparent and node is the
      // left child of parent, perform a right rotation to promote node into
      // parent's place and make parent node's right child. The node and
      // parent references are then swapped. Node is now parent's right child
      // and parent is grandparent's right child.
      if parent.children[1 - direction] == node_index {
        self.rotate(parent_index, 1 - direction);

        // Swap the references, NOT the values.
        mem::swap(&mut node, &mut parent);
      }

      // Finally, promote node into grandparent's position and update the
      // colors to satisfy rules 3 and 4.
      self.rotate(grandparent_index, direction);
      parent.color = Color::BLACK;
      grandparent.color = Color::RED;

      break;
    }

    // Just unconditionally make the root black to always satisfy rule 5. The
    // root cannot be the sentinel.
    let root = Self::get_node_unchecked_mut(vec_ptr, self.root);
    root.color = Color::BLACK;
  }

  /// Restore red-black properties after removing a node.
  ///
  /// # Parameters
  ///
  /// * `index` - The starting index for traversal.
  ///
  /// # Assumptions
  ///
  /// Assumes `index` is valid or 0.
  fn fixup_after_removal(&mut self, index: usize) {
    // Fixup must have overlapping mutable references to nodes in the tree. The
    // modifications are well-defined, and we do not need the borrow checker to
    // protect us here. Modifications will go through raw pointers.
    let vec_ptr = self.nodes.as_mut_ptr();

    let mut current_index = index;
    let mut current = Self::get_node_unchecked_mut(vec_ptr, current_index);

    // Case 1: We are at the root or the color is red. Either way, a black node
    // has been removed from every path and the red-black properties are fixed.
    while current_index != self.root && current.color == Color::BLACK {
      let parent_index = current.parent;
      let parent = Self::get_node_unchecked_mut(vec_ptr, parent_index);
      let direction = Self::get_node_direction(current_index, parent);

      let mut sibling_index = parent.children[1 - direction];
      let mut sibling = Self::get_node_unchecked_mut(vec_ptr, sibling_index);

      let mut near_index = sibling.children[direction];
      let mut near = Self::get_node_unchecked_mut(vec_ptr, near_index);

      let mut far_index = sibling.children[1 - direction];
      let mut far = Self::get_node_unchecked_mut(vec_ptr, far_index);

      // Case 3: If the sibling is red, then its two child nodes are black.
      // Rotate to promote the sibling above the parent, then make the parent
      // red and the old sibling black.
      //
      //        P                S*             S
      //       / \              / \            / \
      //      C   S*           P   F          P*  F
      //         / \          / \            / \
      //        N   F        C   N          C   N
      //
      // (*) Red Node
      //
      // Case 4, 5, or 6 will take care of the remainder.
      if sibling.color == Color::RED {
        self.rotate(parent_index, 1 - direction);
        parent.color = Color::RED;
        sibling.color = Color::BLACK;

        sibling_index = near_index;
        sibling = Self::get_node_unchecked_mut(vec_ptr, sibling_index);

        near_index = sibling.children[direction];
        near = Self::get_node_unchecked_mut(vec_ptr, near_index);

        far_index = sibling.children[1 - direction];
        far = Self::get_node_unchecked_mut(vec_ptr, far_index);

        // Case 4: These are now the children of the *previous* near node, now
        // the sibling. If both of its child are black, we can simply make the
        // sibling red and the parent black, and we are done.
        if near.color == Color::BLACK && far.color == Color::BLACK {
          sibling.color = Color::RED;
          parent.color = Color::BLACK;
          return;
        }
      }

      if near.color == Color::RED || far.color == Color::RED {
        // Case 5: The near node is red but the far node is black. Promote the
        // near node, then swap the colors to convert to case 6.
        //
        //        P               P              P
        //       / \             / \            / \
        //      C   S           C   N*         C   S
        //         / \               \              \
        //        N*  F               S              F*
        //
        // (*) Red Node
        if far.color == Color::BLACK {
          self.rotate(sibling_index, direction);
          sibling.color = Color::RED;
          near.color = Color::BLACK;

          far_index = sibling_index;
          far = Self::get_node_unchecked_mut(vec_ptr, far_index);

          sibling_index = near_index;
          sibling = Self::get_node_unchecked_mut(vec_ptr, sibling_index);
        }

        // Case 6: The far node is red. Rotate to promote the sibling node. Give
        // the sibling the parent's color, then make the parent and far node
        // black. At this point, it does not matter what color the sibling has.
        // We are done.
        //
        //        P?                S               S?
        //       / \               / \             / \
        //      C   S             P?  F*          P   F
        //           \           /               /
        //            F*        C               C
        //
        // (*) Red Node
        // (?) Unknown Color
        self.rotate(parent_index, 1 - direction);
        sibling.color = parent.color;
        parent.color = Color::BLACK;
        far.color = Color::BLACK;
        return;
      }

      // Case 4 again.
      if parent.color == Color::RED {
        sibling.color = Color::RED;
        parent.color = Color::BLACK;
        return;
      }

      // Case 2. At this point, the sibling's child nodes are black, the sibling
      // is black, and the parent is black. Make the sibling red to remove a
      // black node from the path, and move up the tree.
      sibling.color = Color::RED;
      current_index = parent_index;
      current = Self::get_node_unchecked_mut(vec_ptr, current_index);
    }

    // We reached the root. Just make it black to satisfy rule 5. It does not
    // matter if this is the sentinel node.
    current.color = Color::BLACK;
  }

  /// Rotate a node.
  ///
  /// # Parameters
  ///
  /// * `index` - The node to rotate.
  /// * `direction` - The child to promote.
  ///
  /// # Description
  ///
  /// If `direction` is `RIGHT`, a *left* rotation is performed where B is
  /// promoted above A in the diagram below. If `direction` is `LEFT`, then a
  /// *right* rotation is performed where A is promoted above B.
  ///
  ///            --- Right -->
  ///
  ///        P               P
  ///         \               \
  ///         (B)             (A)
  ///         / \             / \
  ///       (A)  E           C  (B)
  ///       / \                 / \
  ///      C   D               D   E
  ///
  ///            <-- Left  ---
  ///
  /// # Assumptions
  ///
  /// Assumes the node at `index` exists and the child at the specified
  /// `direction` exists. If `direction` itself is invalid, subtraction will
  /// panic.
  fn rotate(&mut self, index: usize, direction: usize) {
    // A rotation must have overlapping mutable references to nodes in the tree.
    // The modifications are well-defined, and we do not need the borrow checker
    // to protect us here. Modifications will go through raw pointers.
    let vec_ptr = self.nodes.as_mut_ptr();

    let node = Self::get_node_unchecked_mut(vec_ptr, index);

    let child_index = node.children[direction];
    let child = Self::get_node_unchecked_mut(vec_ptr, child_index);

    // Using a left rotation in the diagram above as an example, swap B into
    // A's position as a child of P and update A's parent to B. If A is the
    // root of the tree, make B the root.
    child.parent = node.parent;
    node.parent = child_index;
    if child.parent != 0 {
      let parent = Self::get_node_unchecked_mut(vec_ptr, child.parent);
      let direction = Self::get_node_direction(index, parent);
      parent.children[direction] = child_index;
    } else {
      self.root = child_index;
    }

    // Using a left rotation in the diagram above as an example, make B's left
    // child the new right child of A and make A the new left child of B.
    let opposite = 1 - direction;
    if child.children[opposite] != 0 {
      let grandchild = Self::get_node_unchecked_mut(vec_ptr, child.children[opposite]);
      grandchild.parent = index;
    }

    node.children[direction] = child.children[opposite];
    child.children[opposite] = index;
  }

  /// Helper to get an unchecked mutable reference to a node.
  ///
  /// # Parameters
  ///
  /// * `vec_ptr` - The raw vector pointer.
  /// * `index` - The node index in the vector.
  ///
  /// # Assumptions
  ///
  /// Assumes `vec_ptr` is valid and `index` is a valid.
  ///
  /// # Returns
  ///
  /// A mutable reference to the node.
  fn get_node_unchecked_mut<'a>(
    vec_ptr: *mut Option<Node<K, V>>,
    index: usize,
  ) -> &'a mut Node<K, V> {
    let node_ptr = unsafe { vec_ptr.add(index) };
    unsafe { (*node_ptr).as_mut().unwrap() }
  }

  /// Helper to get a node's direction relative to its parent.
  ///
  /// # Parameters
  ///
  /// * `index` - The node index.
  /// * `parent` - The parent node.
  ///
  /// # Assumptions
  ///
  /// Assumes `index` *is* a child of `parent`.
  ///
  /// # Returns
  ///
  /// LEFT if the node is the parent's left child, otherwise RIGHT.
  fn get_node_direction(index: usize, parent: &Node<K, V>) -> usize {
    if parent.children[LEFT] == index {
      return LEFT;
    }

    RIGHT
  }

  /// Get the next index using in-order traversal.
  ///
  /// # Parameters
  ///
  /// * `index` - The current index in the traversal.
  /// * `previous` - The previous index in the traversal.
  ///
  /// # Description
  ///
  /// The traversal may start from any valid index in the tree and will only
  /// visit nodes with a key greater than or equal to the key at the starting
  /// index.
  ///
  /// On the first call, previous must be set to 0 to indicate that the left
  /// subtree should not be traversed. Subsequent calls must pass in the values
  /// returned from the prior call.
  ///
  /// # Returns
  ///
  /// A tuple with the new current and previous indices, or None if there are no
  /// more nodes left in the traversal.
  fn next_in_order(&self, index: usize, previous: usize) -> Option<(usize, usize)> {
    if index == 0 || index >= self.nodes.len() {
      return None;
    }

    let mut new_index = index;
    let mut new_previous = previous;

    let mut node = self.nodes[new_index].as_ref()?;

    if node.children[LEFT] == 0 || new_previous == node.children[LEFT] || new_previous == 0 {
      if node.children[RIGHT] == 0 {
        new_previous = index;
        new_index = node.parent;

        while new_index != 0 {
          node = self.nodes[new_index].as_ref().unwrap();

          if new_previous != node.children[RIGHT] {
            break;
          }

          new_previous = new_index;
          new_index = node.parent;
        }

        return Some((new_index, new_previous));
      }

      new_previous = index;
      new_index = node.children[RIGHT];
      node = self.nodes[new_index].as_ref().unwrap();
    }

    while node.children[LEFT] != 0 {
      new_previous = new_index;
      new_index = node.children[LEFT];
      node = self.nodes[new_index].as_ref().unwrap();
    }

    Some((new_index, new_previous))
  }
}

/// In-order tree iterator.
pub struct InOrderIterator<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  tree: &'tree RedBlackTree<'alloc, A, K, V>,
  current_index: usize,
  previous_index: usize,
}

impl<'tree, 'alloc, A, K, V> InOrderIterator<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  /// Construct a new InOrderIterator.
  pub fn new(tree: &'tree RedBlackTree<'alloc, A, K, V>) -> Self {
    let start = if tree.root != 0 {
      tree.binary_tree_minimum(tree.root)
    } else {
      0
    };

    Self {
      tree,
      current_index: start,
      previous_index: 0,
    }
  }
}

impl<'tree, 'alloc, A, K, V> Iterator for InOrderIterator<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  type Item = (&'tree K, &'tree V);

  /// See `Iterator::next()`.
  fn next(&mut self) -> Option<Self::Item> {
    if self.current_index == 0 {
      return None;
    }

    let index = self.current_index;

    (self.current_index, self.previous_index) = self
      .tree
      .next_in_order(self.current_index, self.previous_index)
      .unwrap_or((0, 0));

    let ptr = unsafe { self.tree.nodes.as_ptr() };
    let current = unsafe { (*ptr.add(index)).as_ref()? };
    Some((&current.key, &current.value))
  }
}

/// In-order tree iterator that allows mutation of the value.
pub struct InOrderIteratorMut<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  tree: &'tree mut RedBlackTree<'alloc, A, K, V>,
  current_index: usize,
  previous_index: usize,
}

impl<'tree, 'alloc, A, K, V> InOrderIteratorMut<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  /// Construct a new InOrderIteratorMut.
  pub fn new(tree: &'tree mut RedBlackTree<'alloc, A, K, V>) -> Self {
    let start = if tree.root != 0 {
      tree.binary_tree_minimum(tree.root)
    } else {
      0
    };

    Self {
      tree,
      current_index: start,
      previous_index: 0,
    }
  }
}

impl<'tree, 'alloc, A, K, V> Iterator for InOrderIteratorMut<'tree, 'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  type Item = (&'tree K, &'tree mut V);

  /// See `Iterator::next()`.
  fn next(&mut self) -> Option<Self::Item> {
    if self.current_index == 0 {
      return None;
    }

    let index = self.current_index;

    (self.current_index, self.previous_index) = self
      .tree
      .next_in_order(self.current_index, self.previous_index)
      .unwrap_or((0, 0));

    let ptr = unsafe { self.tree.nodes.as_mut_ptr() };
    let current = unsafe { (*ptr.add(index)).as_mut()? };
    Some((&current.key, &mut current.value))
  }
}

impl<'tree, 'alloc, A, K, V> IntoIterator for &'tree RedBlackTree<'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  type Item = (&'tree K, &'tree V);
  type IntoIter = InOrderIterator<'tree, 'alloc, A, K, V>;

  /// See `IntoIterator::into_iter()`.
  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

impl<'tree, 'alloc, A, K, V> IntoIterator for &'tree mut RedBlackTree<'alloc, A, K, V>
where
  A: PageAllocator,
  K: Sized + PartialOrd,
{
  type Item = (&'tree K, &'tree mut V);
  type IntoIter = InOrderIteratorMut<'tree, 'alloc, A, K, V>;

  /// See `IntoIterator::into_iter()`.
  fn into_iter(self) -> Self::IntoIter {
    self.iter_mut()
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut crate::test::TestContext) {
  tests::run_tests(context);
}
