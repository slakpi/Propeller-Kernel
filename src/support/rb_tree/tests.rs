//! Red-Black Tree Tests

use crate::arch;
use crate::support::memory::PageAllocator;
use crate::support::rb_tree::{Color, LEFT, Node, RIGHT, RedBlackTree};
use crate::sync::SpinLock;
use crate::test::memory::{TestPageAllocator, make_test_page_allocator, reset_test_memory};
use crate::{
  check, check_eq, check_gt, check_lt, check_not_none, check_optional, check_result, debug_print,
  execute_test, mark_fail, test_module,
};
use crate::{check_none, test};
use core::cmp::{Ordering, PartialEq, PartialOrd};
use core::fmt::Display;
use core::iter::IntoIterator;
use core::mem;

impl Display for Color {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Color::RED => write!(f, "RED"),
      Color::BLACK => write!(f, "BLACK"),
    }
  }
}

/// Data structure for the test tree.
struct VerificationData {
  black_height: usize,
  child_sum: u32,
  descendants: usize,
  tag: u32,
}

impl VerificationData {
  /// Construct a new verification data node.
  fn new() -> Self {
    Self {
      black_height: 0,
      child_sum: 0,
      descendants: 0,
      tag: 0,
    }
  }
}

/// Convenience type to keep test tree definitions consistent.
type TestTree<'alloc> = RedBlackTree<'alloc, TestPageAllocator, u32, VerificationData>;

/// Red-black tree rule violations.
enum TreeViolation {
  RedRedViolation,
  BlackPathViolation,
  NonBlackRoot,
}

impl Display for TreeViolation {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      TreeViolation::RedRedViolation => write!(f, "Red parent with red child"),
      TreeViolation::BlackPathViolation => write!(f, "Black path violation"),
      TreeViolation::NonBlackRoot => write!(f, "Non-black root violation"),
    }
  }
}

fn verify_tree(tree: &mut TestTree) -> Result<(), TreeViolation> {
  verify_tree_with_output(tree, false)
}

/// Traverse the entire tree and verify it satisfies the red-black tree rules.
///
/// # Parameters
///
/// * `tree` - The tree to verify.
///
/// # Description
///
/// Performs a post-order traversal using the verification data in each node to
/// track whether the tree satisfies the red-black tree rules.
///
/// See `RedBlackTree::balance`.
///
/// # Returns
///
/// Ok if the tree is valid, or a `TreeViolation` otherwise.
fn verify_tree_with_output(tree: &mut TestTree, print: bool) -> Result<(), TreeViolation> {
  let mut current_index = tree.root;
  let mut last = 0;
  let vec_ptr = tree.nodes.as_mut_ptr();

  loop {
    if current_index == 0 {
      break;
    }

    let new_last = current_index;

    let node_ptr = unsafe { vec_ptr.add(current_index) };
    let node = unsafe { (*node_ptr).as_mut().unwrap() };

    // Verify the root is black.
    if current_index == tree.root && node.color != Color::BLACK {
      return Err(TreeViolation::NonBlackRoot);
    }

    // * If the current node has a right child AND we arrived back here from the
    //   right child, post-order verify this node and return to the parent.
    // * If this node does not have a left child OR we arrived back here from
    //   the left child, check if the node has a right child. If it does, move
    //   to the right. Otherwise, post-order verify this node and return to the
    //   parent.
    // * Otherwise, move left.
    if node.children[RIGHT] != 0 && last == node.children[RIGHT] {
      verify_node(tree, current_index, node, print)?;
      current_index = node.parent;
    } else if node.children[LEFT] == 0 || last == node.children[LEFT] {
      if node.children[RIGHT] != 0 {
        current_index = node.children[RIGHT];
      } else {
        verify_node(tree, current_index, node, print)?;
        current_index = node.parent;
      }
    } else {
      current_index = node.children[LEFT];
    }

    last = new_last;
  }

  if print && tree.nodes.len() > 0 {
    let node = tree.nodes[0].as_ref().unwrap();
    debug_print!(
      "0 {}: V {}, P {:?}, L {:?}, R {:?}\n",
      node.color,
      node.key,
      node.parent,
      node.children[LEFT],
      node.children[RIGHT],
    );
  }

  Ok(())
}

/// Helper for `verify_tree` to update and verify a node.
///
/// # Parameters
///
/// * `tree` - The tree being verified.
/// * `node` - The node to verify.
///
/// # Description
///
/// Verifies:
///
/// * Red nodes do not have any red children.
/// * All paths to the node's leaves have the same number of black nodes.
///
/// # Returns
///
/// Ok if the node is valid, or a `TreeViolation` otherwise.
fn verify_node(
  tree: &TestTree,
  index: usize,
  node: &mut Node<u32, VerificationData>,
  print: bool,
) -> Result<(), TreeViolation> {
  let black = if node.color == Color::BLACK { 1 } else { 0 };
  let mut height: Option<usize> = None;

  node.value.child_sum = 0;
  node.value.descendants = 0;

  if print {
    debug_print!(
      "{} {}: V {}, P {:?}, L {:?}, R {:?}\n",
      index,
      node.color,
      node.key,
      node.parent,
      node.children[LEFT],
      node.children[RIGHT],
    );
  }

  for child_index in node.children {
    if child_index == 0 {
      continue;
    }

    let child = tree.nodes[child_index].as_ref().unwrap();

    if let Some(height) = height {
      if child.value.black_height != height {
        return Err(TreeViolation::BlackPathViolation);
      }
    } else {
      height = Some(child.value.black_height);
    }

    if node.color == Color::RED && child.color == Color::RED {
      return Err(TreeViolation::RedRedViolation);
    }

    node.value.child_sum += child.value.child_sum + child.key;
    node.value.descendants += child.value.descendants + 1;
  }

  node.value.black_height = if let Some(height) = height {
    height + black
  } else {
    black
  };

  Ok(())
}

/// Convenience function to get a nodes color without a copy.
///
/// # Parameters
///
/// * `tree` - The tree that contains the node.
/// * `index` - The index of the node.
///
/// # Assumptions
///
/// Assumes the node exists.
///
/// # Returns
///
/// The color of the node.
fn get_node_color<'alloc>(tree: &TestTree, index: usize) -> Color {
  match tree.nodes[index].as_ref().unwrap().color {
    Color::RED => Color::RED,
    Color::BLACK => Color::BLACK,
  }
}

/// Run the red-black tree tests.
///
/// # Parameters
///
/// * `context` - The test context.
pub fn run_tests(context: &mut test::TestContext) {
  test_module!(rb_tree);
  execute_test!(context, rb_tree, test_initialization);
  execute_test!(context, rb_tree, test_insert_single);
  execute_test!(context, rb_tree, test_insert_left_pair);
  execute_test!(context, rb_tree, test_insert_right_pair);
  execute_test!(context, rb_tree, test_insert_three_left_chain);
  execute_test!(context, rb_tree, test_insert_three_right_chain);
  execute_test!(context, rb_tree, test_insert_lr_rotation);
  execute_test!(context, rb_tree, test_insert_rl_rotation);
  execute_test!(context, rb_tree, test_insert_large_sorted);
  execute_test!(context, rb_tree, test_insert_large_unsorted);
  execute_test!(context, rb_tree, test_insert_duplicate_values);
  execute_test!(context, rb_tree, test_drop_frees_memory);
  execute_test!(context, rb_tree, test_alloc_fail_during_insert);
  execute_test!(context, rb_tree, test_remove_leaf);
  execute_test!(context, rb_tree, test_remove_root);
  execute_test!(context, rb_tree, test_remove_all);
  execute_test!(context, rb_tree, test_remove_all_sorted);
  execute_test!(context, rb_tree, test_empty);
  execute_test!(context, rb_tree, test_remove_invalid_index);
  execute_test!(context, rb_tree, test_remove_empty_tree);
  execute_test!(context, rb_tree, test_node_reuse);
  execute_test!(context, rb_tree, test_retrieving_values);
  execute_test!(context, rb_tree, test_find);
  execute_test!(context, rb_tree, test_lower_bound);
  execute_test!(context, rb_tree, test_upper_bound);
  execute_test!(context, rb_tree, test_iterator);
  execute_test!(context, rb_tree, test_mutable_iterator);
}

/// Verify a newly created tree is empty with no memory allocated.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_initialization(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let free_mem = allocator.lock().get_free_mem();
  let tree = TestTree::new(&allocator);

  check_eq!(context, tree.is_empty(), true);
  check_eq!(context, allocator.lock().get_free_mem(), free_mem);
  check_eq!(context, allocator.lock().get_alloc_count(), 0);
}

/// Test inserting a single node.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_single(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  check_eq!(context, tree.is_empty(), true);

  _ = tree.insert(42, VerificationData::new()).unwrap();
  check!(context, !tree.is_empty());

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify inserting a left child.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_left_pair(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let i2 = tree.insert(2, VerificationData::new()).unwrap();
  let i1 = tree.insert(1, VerificationData::new()).unwrap();

  check_eq!(context, tree.root, i2);

  // Verify the structure of the tree.
  let root = tree.nodes[i2].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], i1);
  check_eq!(context, root.children[RIGHT], 0);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify inserting a right child.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_right_pair(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  let i3 = tree.insert(3, VerificationData::new()).unwrap();

  check_eq!(context, tree.root, i1);

  // Verify the structure of the tree.
  let root = tree.nodes[i1].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], 0);
  check_eq!(context, root.children[RIGHT], i3);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify a LL double-red resolves to a balanced tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_three_left_chain(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // This patterned would create a left chain in a normal BST.
  let i3 = tree.insert(3, VerificationData::new()).unwrap();
  let i2 = tree.insert(2, VerificationData::new()).unwrap();
  let i1 = tree.insert(1, VerificationData::new()).unwrap();

  // A right rotation balances the tree.
  check_eq!(context, tree.root, i2);
  check_eq!(context, get_node_color(&tree, i2), Color::BLACK);
  check_eq!(context, get_node_color(&tree, i1), Color::RED);
  check_eq!(context, get_node_color(&tree, i3), Color::RED);

  // Verify the structure of the tree.
  let root = tree.nodes[i2].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], i1);
  check_eq!(context, root.children[RIGHT], i3);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify a RR double-red resolves to a balanced tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_three_right_chain(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // This patterned would create a right chain in a normal BST.
  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  let i2 = tree.insert(2, VerificationData::new()).unwrap();
  let i4 = tree.insert(4, VerificationData::new()).unwrap();

  // A left rotation balances the tree.
  check_eq!(context, tree.root, i2);
  check_eq!(context, get_node_color(&tree, i2), Color::BLACK);
  check_eq!(context, get_node_color(&tree, i1), Color::RED);
  check_eq!(context, get_node_color(&tree, i4), Color::RED);

  // Verify the structure of the tree.
  let root = tree.nodes[i2].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], i1);
  check_eq!(context, root.children[RIGHT], i4);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify a RL double-red resolves to a balanced tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_rl_rotation(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // This patterned would create a right/left chain in a normal BST.
  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  let i3 = tree.insert(3, VerificationData::new()).unwrap();
  let i2 = tree.insert(2, VerificationData::new()).unwrap();

  // A right rotation followed by a left rotation balances the tree.
  check_eq!(context, tree.root, i2);
  check_eq!(context, get_node_color(&tree, i2), Color::BLACK);
  check_eq!(context, get_node_color(&tree, i1), Color::RED);
  check_eq!(context, get_node_color(&tree, i3), Color::RED);

  // Verify the structure of the tree.
  let root = tree.nodes[i2].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], i1);
  check_eq!(context, root.children[RIGHT], i3);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify a LR double-red resolves to balanced tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_lr_rotation(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // This patterned would create a right/left chain in a normal BST.
  let i4 = tree.insert(4, VerificationData::new()).unwrap();
  let i2 = tree.insert(2, VerificationData::new()).unwrap();
  let i3 = tree.insert(3, VerificationData::new()).unwrap();

  // A left rotation followed by a right rotation balances the tree.
  check_eq!(context, tree.root, i3);
  check_eq!(context, get_node_color(&tree, i3), Color::BLACK);
  check_eq!(context, get_node_color(&tree, i2), Color::RED);
  check_eq!(context, get_node_color(&tree, i4), Color::RED);

  // Verify the structure of the tree.
  let root = tree.nodes[i3].as_ref().unwrap();
  check_eq!(context, root.children[LEFT], i2);
  check_eq!(context, root.children[RIGHT], i4);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify inserting 1 through 15 in sorted order.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_large_sorted(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  for i in 1..=15 {
    check_not_none!(context, tree.insert(i, VerificationData::new()));

    // Verify the properties of the tree.
    check_result!(context, verify_tree(&mut tree));
  }
}

/// Verify inserting an unsorted sequence that exercises rotation patterns.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// Red nodes are marked with (*).
///
/// Insert 41:
///
///       41
///
/// Insert 38:
///
///          41
///         /
///       38*
///
/// Insert 31: LL double-red, right rotation
///
///            41              38
///           /               /  \
///         38*             31*  41*
///         /
///       31*
///
/// Insert 12: Red parent with red sibling, recolor parent and parent sibling
/// black, recolor grandparent red. Recolor root black
///
///            38                38
///           /  \              /  \
///         31*  41*           31  41
///         /                 /
///       12*               12*
///
/// Insert 19: LR double-red, left rotate, recolor, right-rotate
///
///              38              38
///             /  \            /  \
///            31  41          19  41
///           /               /  \
///         12*             12*  31*
///         /
///       19*
///
/// Insert 8: Red parent with red sibling, recolor parent and parent sibling
/// black, recolor grandparent red.
///
///             38                38
///            /  \              /  \
///           19  41           19*  41
///          /  \              /  \
///        12*  31*           12  31
///        /                 /
///       8*                8*
///
/// Insert 50: No fixup necessary.
///
///              38
///             /  \
///           19*  41
///          /  \    \
///         12  31   50*
///        /
///       8*
///
/// Insert 55: RR double-red, left rotation
///
///              38                       38
///             /  \                     /  \
///           19*  41                   /    \
///          /  \    \                19*    50
///         12  31   50*             /  \   /  \
///        /           \            12  31 41* 55*
///       8*           55*         /
///                               8*
///
/// Insert 60: Red parent with red sibling, recolor parent and parent sibling
/// black, recolor grandparent red.
///
///               38                      38
///              /  \                    /  \
///             /    \                  /    \
///           19*    50               19*    50*
///          /  \   /  \             /  \   /  \
///         12  31 41* 55*          12  31 41  55
///        /                       /             \
///       8*                      8*             60*
///
/// Insert 57: RL double-red, right rotate, recolor, RR double-red, left rotate.
///
///               38                      38
///              /  \                    /  \
///             /    \                  /    \
///           19*    50*              19*    50*
///          /  \   /  \             /  \   /  \
///         12  31 41  55           12  31 41  57
///        /             \         /          /  \
///       8*             60*      8*        55*  60*
///                     /
///                   57*
fn test_insert_large_unsorted(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));

    // Verify the properties of the tree.
    check_result!(context, verify_tree(&mut tree));
  }
}

/// Verify that inserting a duplicate value goes to the right subtree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_duplicate_values(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let index_first = tree.insert(5, VerificationData::new()).unwrap();
  let index_dup = tree.insert(5, VerificationData::new()).unwrap();

  // Verify duplicate is in the right subtree.
  check_eq!(context, tree.root, index_first);
  let root = tree.nodes[tree.root].as_ref().unwrap();
  check_eq!(context, root.color, Color::BLACK);
  check_eq!(context, root.children[LEFT], 0);
  check_eq!(context, root.children[RIGHT], index_dup);
  check_eq!(context, get_node_color(&tree, index_dup), Color::RED);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify that the tree's allocated memory is freed when dropped.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_drop_frees_memory(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let free_mem = allocator.lock().get_free_mem();

  {
    let mut tree = TestTree::new(&allocator);

    for i in 0..10 {
      check_not_none!(context, tree.insert(i, VerificationData::new()));
    }

    let lock = allocator.lock();
    check_eq!(context, lock.get_alloc_count(), 1);
    check_lt!(context, lock.get_free_mem(), free_mem);
  }

  check_eq!(context, allocator.lock().get_free_mem(), free_mem);
}

/// Verify the tree is unmodified if an insertion fails.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_alloc_fail_during_insert(context: &mut test::TestContext) {
  // Calculate the maximum number of items to insert to fill a single page. Take
  // the sentinel value into account by subtracting one.
  let max = arch::get_page_size() / mem::size_of::<Node<u32, VerificationData>>() - 1;

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // Insert enough nodes to fill a page.
  for i in 0..max {
    check_not_none!(context, tree.insert(i as u32, VerificationData::new()));
  }

  // Verify the tree to update the stats.
  check_result!(context, verify_tree(&mut tree));

  // Save the stats.
  let root = tree.nodes[tree.root].as_ref().unwrap();
  let child_sum = root.value.child_sum;
  let descendants = root.value.descendants;

  // Freeze the allocator and attempt to insert.
  allocator.lock().set_can_alloc(false);
  check_none!(context, tree.insert(99, VerificationData::new()));

  // Verify the tree to update the stats.
  check_result!(context, verify_tree(&mut tree));

  // The tree should be unchanged.
  let root = tree.nodes[tree.root].as_ref().unwrap();
  check_eq!(context, root.value.child_sum, child_sum);
  check_eq!(context, root.value.descendants, descendants);

  // Unfreeze and attempt another insert.
  allocator.lock().set_can_alloc(true);
  check_not_none!(context, tree.insert(99, VerificationData::new()));

  // Verify the tree to update the stats.
  check_result!(context, verify_tree(&mut tree));

  // The tree should be modified.
  let root = tree.nodes[tree.root].as_ref().unwrap();
  check_eq!(context, root.value.child_sum, child_sum + 99);
  check_eq!(context, root.value.descendants, descendants + 1);
}

/// Verify removing a leaf node.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_leaf(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  let i3 = tree.insert(3, VerificationData::new()).unwrap();

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));

  // Remove the red leaf. This will be a direct removal.
  let removed = tree.remove(i3).unwrap();
  check_eq!(context, removed.0, 3);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));

  // Remove the last leaf node.
  let removed = tree.remove(i1).unwrap();
  check_eq!(context, removed.0, 1);
  check!(context, tree.is_empty());
}

/// Remove the root of a complex tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_root(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));

  // Remove the root.
  check_not_none!(context, tree.remove(tree.root));

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Remove all nodes one-by-one from a small tree, verifying after each step.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_all(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  // Verify the tree has allocated any memory.
  let alloc_mem = allocator.lock().get_alloc_mem();
  check_gt!(context, alloc_mem, 0);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));

  for value in values {
    let ret = tree.erase(value).unwrap();
    check_eq!(context, ret.0, value);

    // Verify the properties of the tree.
    check_result!(context, verify_tree(&mut tree));
  }

  // Verify the tree is empty and that it deallocated its memory.
  check!(context, tree.is_empty());
  check_eq!(context, allocator.lock().get_alloc_mem(), 0);
}

/// Remove all nodes from a large sorted insert sequence.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_all_sorted(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  for i in 0..20 {
    check_not_none!(context, tree.insert(i, VerificationData::new()));
  }

  // Verify the tree has allocated any memory.
  let alloc_mem = allocator.lock().get_alloc_mem();
  check_gt!(context, alloc_mem, 0);

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));

  for i in 0..20 {
    let ret = tree.erase(i).unwrap();
    check_eq!(context, ret.0, i);

    // Verify the properties of the tree.
    check_result!(context, verify_tree(&mut tree));
  }

  // Verify the tree is empty and that it deallocated its memory.
  check!(context, tree.is_empty());
  check_eq!(context, allocator.lock().get_alloc_mem(), 0);
}

/// Verify emptying a tree.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_empty(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  // Verify the tree has allocated any memory.
  let alloc_mem = allocator.lock().get_alloc_mem();
  check_gt!(context, alloc_mem, 0);

  tree.empty();

  // Verify the tree is empty and that it deallocated its memory.
  check!(context, tree.is_empty());
  check_eq!(context, allocator.lock().get_alloc_mem(), 0);
}

/// Verify removing an invalid index returns None.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_invalid_index(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // The sentinel is lazy allocated with the first insert.
  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  check_gt!(context, i1, 0);

  // Attempt to remove invalid indices.
  check_none!(context, tree.remove(0));
  check_none!(context, tree.remove(i1 + 1));

  // Verify the properties of the tree.
  check_result!(context, verify_tree(&mut tree));
}

/// Verify removing from an empty tree returns None.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove_empty_tree(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // No nodes exist, so removal should return None.
  check_none!(context, tree.remove(1));
}

/// Verify node reuse.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_node_reuse(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let i1 = tree.insert(1, VerificationData::new()).unwrap();
  let i2 = tree.insert(2, VerificationData::new()).unwrap();
  let i3 = tree.insert(3, VerificationData::new()).unwrap();
  let len = tree.nodes.len();

  // Remove two of the nodes.
  check_not_none!(context, tree.remove(i1));
  check_not_none!(context, tree.remove(i3));

  // Verify the node vector's length has not changed and that i1 and i3 are now
  // None.
  check_eq!(context, tree.nodes.len(), len);
  check_none!(context, tree.nodes[i1]);
  check_none!(context, tree.nodes[i3]);

  // Verify i1 and i3 are in the free list.
  check_eq!(context, tree.free.len(), 2);
  check_eq!(context, tree.free[0], i1);
  check_eq!(context, tree.free[1], i3);

  // Add another node. The free list is a stack, so it should get index i3 and
  // the free list should decrease by one.
  check_optional!(context, tree.insert(42, VerificationData::new()), i3);
  check_eq!(context, tree.free.len(), 1);

  // Reuse the last free node.
  check_optional!(context, tree.insert(99, VerificationData::new()), i1);
  check!(context, tree.free.is_empty());
}

/// Verify retrieving values.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_retrieving_values(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [1, 2, 3];
  for v in values {
    let mut data = VerificationData::new();
    data.tag = v;
    check_not_none!(context, tree.insert(v, data));
  }

  let i1 = tree.find(1).unwrap();
  let i2 = tree.find(2).unwrap();
  let i3 = tree.find(3).unwrap();

  // Test invalid indices.
  check_eq!(context, tree.nodes.len(), 4);
  check_none!(context, tree.get_value(0));
  check_none!(context, tree.get_value(4));

  // Test valid indices with mutable retrieval.
  let n = tree.get_value_mut(i1).unwrap();
  check_eq!(context, n.tag, 1);
  n.tag = 10;
  let n = tree.get_value_mut(i2).unwrap();
  check_eq!(context, n.tag, 2);
  n.tag = 20;
  let n = tree.get_value_mut(i3).unwrap();
  check_eq!(context, n.tag, 3);
  n.tag = 30;

  // Test valid indices with immutable retrieval.
  let n = tree.get_value(i1).unwrap();
  check_eq!(context, n.tag, 10);
  let n = tree.get_value(i2).unwrap();
  check_eq!(context, n.tag, 20);
  let n = tree.get_value(i3).unwrap();
  check_eq!(context, n.tag, 30);
}

/// Verify find.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_find(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  // Test searching for values that exist.
  for i in 1..=values.len() {
    check_optional!(context, tree.find(values[i - 1]), i);
  }

  // Test searching for a value not in the tree.
  check_none!(context, tree.find(99));
}

/// Verify lower bound.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_lower_bound(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  let tests = [
    (40, Some(1)),
    (41, Some(1)),
    (42, Some(7)),
    (7, Some(6)),
    (8, Some(6)),
    (9, Some(4)),
    (30, Some(3)),
    (31, Some(3)),
    (32, Some(2)),
    (59, Some(9)),
    (60, Some(9)),
    (61, None),
  ];

  for test in tests {
    let ret = tree.lower_bound(test.0);
    check!(context, ret == test.1);
  }
}

/// Verify upper bound.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_upper_bound(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    check_not_none!(context, tree.insert(v, VerificationData::new()));
  }

  let tests = [
    (40, Some(1)),
    (41, Some(7)),
    (42, Some(7)),
    (7, Some(6)),
    (8, Some(4)),
    (9, Some(4)),
    (30, Some(3)),
    (31, Some(2)),
    (32, Some(2)),
    (59, Some(9)),
    (60, None),
    (61, None),
  ];

  for test in tests {
    let ret = tree.upper_bound(test.0);
    check!(context, ret == test.1);
  }
}

/// Verify iterator.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_iterator(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // Test iterator with an empty tree.
  for _ in tree.iter() {
    mark_fail!(context, "Iterator should not have yielded a value.");
  }

  // Test iterator with a complex tree.
  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    let mut data = VerificationData::new();
    data.tag = v;
    check_not_none!(context, tree.insert(v, data));
  }

  let order = [8, 12, 19, 31, 38, 41, 50, 55, 57, 60];
  let mut i = 0;
  for kv in tree.iter() {
    check_eq!(context, *kv.0, order[i]);
    check_eq!(context, kv.1.tag, *kv.0);
    i += 1;
  }
}

/// Verify mutable iteration.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_mutable_iterator(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let mut tree = TestTree::new(&allocator);

  // Test iterator with an empty tree.
  for _ in tree.iter_mut() {
    mark_fail!(context, "Iterator should not have yielded a value.");
  }

  // Test iterator with a complex tree and modify values along the way.
  let values = [41, 38, 31, 12, 19, 8, 50, 55, 60, 57];
  for v in values {
    let mut data = VerificationData::new();
    data.tag = v;
    check_not_none!(context, tree.insert(v, data));
  }

  let order = [8, 12, 19, 31, 38, 41, 50, 55, 57, 60];
  let mut i = 0;
  for kv in tree.iter_mut() {
    kv.1.tag += 1;
    i += 1;
  }

  i = 0;
  for kv in tree.iter() {
    check_eq!(context, kv.1.tag, order[i] + 1);
    i += 1;
  }
}
