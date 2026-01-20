//! Range Set Utilities

use super::range::{Range, RangeOrdering};

/// Fixed-size, ordered set of Ranges.
#[derive(Copy, Clone)]
pub struct RangeSet<const SET_SIZE: usize, TagType>
where
  TagType: Copy,
{
  ranges: [Range<TagType>; SET_SIZE],
  count: usize,
}

impl<const SET_SIZE: usize, TagType> RangeSet<SET_SIZE, TagType>
where
  TagType: Copy,
{
  /// Construct a new RangeSet.
  pub const fn new(default_tag: TagType) -> Self {
    Self {
      ranges: [Range {
        tag: default_tag,
        base: 0,
        size: 0,
      }; SET_SIZE],
      count: 0,
    }
  }

  /// Clear the range set.
  pub fn clear(&mut self) {
    self.count = 0;
  }

  /// Check if the set is empty.
  pub fn is_empty(&self) -> bool {
    self.count == 0
  }

  /// Get the length of the set.
  pub fn len(&self) -> usize {
    self.count
  }

  /// Access the ranges.
  pub fn get_ranges(&self) -> &[Range<TagType>] {
    &self.ranges[..self.count]
  }

  /// Insert a new range in to the set ordered by base.
  ///
  /// # Parameters
  ///
  /// * `range` - The new range to add to the set.
  ///
  /// # Description
  ///
  /// Ranges with the same base are ordered from first to last inserted. Ranges
  /// with a size of zero or a size that would overflow are ignored.
  ///
  /// # Returns
  ///
  /// True if able to insert the new range, false otherwise.
  pub fn insert_range(&mut self, range: Range<TagType>) -> bool {
    if self.count >= SET_SIZE {
      return false;
    }

    if range.size == 0 || (usize::MAX - range.size) + 1 < range.base {
      return false;
    }

    let mut ins = self.count;

    for i in 0..self.count {
      if range.base < self.ranges[i].base {
        ins = i;
        break;
      }
    }

    self.ranges.copy_within(ins..self.count, ins + 1);
    self.ranges[ins] = range;
    self.count += 1;

    true
  }

  /// Exclude a range from the set.
  ///
  /// # Parameters
  ///
  /// * `excl` - The range to exclude.
  pub fn exclude_range(&mut self, excl: &Range<TagType>) {
    let mut i = 0usize;

    while i < self.count {
      let Ok(split) = self.ranges[i].exclude(excl) else {
        return;
      };

      // If the first element is valid, the current range can simply be
      // replaced.
      if let Some(a) = split.0 {
        self.ranges[i] = a;
      }

      // If the second element is valid, but the first is not, simply replace
      // the current range. Otherwise, insert the new range after the current
      // range. If inserting, increment the index an extra time.
      if let Some(b) = split.1 {
        if split.0.is_none() {
          self.ranges[i] = b;
        } else if self.count < SET_SIZE {
          self.ranges.copy_within(i..self.count, i + 1);
          self.ranges[i + 1] = b;
          self.count += 1;
          i += 1;
        } else {
          panic!("Could not split range; set is full.");
        }
      }

      // If neither element is valid, remove the current range. Do not increment
      // the index yet.
      if split.0.is_none() && split.1.is_none() {
        self.ranges.copy_within((i + 1)..self.count, i);
        self.count -= 1;
        continue;
      }

      i += 1;
    }

    self.trim_empty_ranges();
  }

  /// Combines ranges as necessary to ensure ranges do not overlap and removes
  /// any empty ranges.
  pub fn trim_ranges(&mut self) {
    self.trim_overlapping_ranges();
    self.trim_empty_ranges();
  }

  /// Removes empty ranges from the set.
  fn trim_empty_ranges(&mut self) {
    let mut i = 0usize;

    while i < self.count {
      if self.ranges[i].size > 0 {
        i += 1;
        continue;
      }

      self.ranges.copy_within((i + 1)..self.count, i);
      self.count -= 1;
    }
  }

  /// Removes overlapping ranges from the set.
  fn trim_overlapping_ranges(&mut self) {
    if self.count < 2 {
      return;
    }

    let mut i = 0usize;

    while i < self.count - 1 {
      // Just unwrap the comparison. The interface ensures the ranges within the
      // set are valid.
      match self.ranges[i].cmp(&self.ranges[i + 1]).unwrap() {
        RangeOrdering::Equal | RangeOrdering::Superset => {
          // This range contains the next range, remove the next range.
          self.ranges.copy_within((i + 2)..self.count, i + 1);
        }

        RangeOrdering::Subset => {
          // The next range contains this range, remove this range.
          self.ranges.copy_within((i + 1)..self.count, i);
        }

        RangeOrdering::LessEqual | RangeOrdering::GreaterEqual => {
          // This range overlaps the next. Union the ranges and remove the
          // extraneous range. Given that we know the ranges are sorted and
          // overlap exists, the unsigned math is safe.
          self.ranges[i].size =
            (self.ranges[i + 1].base + self.ranges[i + 1].size) - self.ranges[i].base;
          self.ranges.copy_within((i + 2)..self.count, i + 1);
        }

        // No overlap, move ahead.
        _ => i += 1,
      }
    }
  }
}
