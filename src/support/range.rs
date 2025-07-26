//! Range Utilities

/// Range ordering.
///
/// * `Less` - The LHS is fully to the left of the RHS.
/// * `LessEqual` - The LHS partially overlaps the beginning of the RHS.
/// * `GreaterEqual` - The LHS partially overlaps the end of the RHS.
/// * `Greater` - The LHS is fully to the right of the RHS.
/// * `Equal` - The two ranges are exactly equal.
/// * `Superset` - The LHS fully contains the RHS.
/// * `Subset` - The LHS is fully contained by the RHS.
pub enum RangeOrdering {
  Less,
  LessEqual,
  GreaterEqual,
  Greater,
  Equal,
  Superset,
  Subset,
}

/// A contiguous range of values in the interval `[base, base + size)`.
#[derive(Copy, Clone)]
pub struct Range {
  pub base: usize,
  pub size: usize,
}

impl Range {
  /// Compare two ranges.
  ///
  /// # Parameters
  ///
  /// * `rhs` - The range to compare against.
  ///
  /// # Returns
  ///
  /// A range ordering or Err if the ranges are invalid.
  pub fn cmp(&self, rhs: &Range) -> Option<RangeOrdering> {
    if self.size == 0 || rhs.size == 0 {
      return None;
    }

    let lhs_end = self.base + (self.size - 1);
    let rhs_end = rhs.base + (rhs.size - 1);

    if self.base == rhs.base && self.size == rhs.size {
      // lhs  |---------------|
      // rhs  |---------------|
      return Some(RangeOrdering::Equal);
    } else if lhs_end < rhs.base {
      // lhs  |-----|
      // rhs         |---------------|
      return Some(RangeOrdering::Less);
    } else if rhs_end < self.base {
      // lhs                   |-----|
      // rhs  |---------------|
      return Some(RangeOrdering::Greater);
    } else if self.base <= rhs.base && lhs_end >= rhs_end {
      // lhs  |---------------|
      // rhs   |-----|
      return Some(RangeOrdering::Superset);
    } else if rhs.base <= self.base && rhs_end >= lhs_end {
      // lhs           |-----|
      // rhs  |---------------|
      return Some(RangeOrdering::Subset);
    } else if self.base < rhs.base {
      // lhs  |-----|
      // rhs     |---------------|
      return Some(RangeOrdering::LessEqual);
    } else {
      // lhs               |-----|
      // rhs  |---------------|
      return Some(RangeOrdering::GreaterEqual);
    }
  }

  /// Splits a range using an exclusion range.
  ///
  /// # Parameters
  ///
  /// * `excl` - The range to exclude.
  ///
  /// # Details
  ///
  /// * If the ranges are mutually exclusive, returns the original range as the
  ///   first element in the tuple and None for the second.
  ///
  /// * If the exclusion range fully encompasses the range, returns None for
  ///   both elements of the tuple.
  ///
  /// * If the base of the exclusion range is greater than the range base,
  ///   returns a new range in the first element of the tuple with the original
  ///   base and a new size calculated using the exclusion range base as the
  ///   end. Otherwise, returns None in the first element of the tuple.
  ///
  ///   If the end of the exclusion range is less than the range end, returns a
  ///   new range in the second element of the tuple with the exclusion range
  ///   base as the base with a new size calculated using the original end.
  ///   Otherwise, returns None in the second element of the tuple.
  ///
  /// The last case handles the exclusion range being fully encompassed by the
  /// range as well as the exclusion range overlapping either end of the range
  /// and handles returning None if the overlap results in empty ranges.
  ///
  /// # Returns
  ///
  /// A tuple with the resulting range(s) of the split. See details.
  pub fn split_range(&self, excl: &Range) -> Result<(Option<Range>, Option<Range>), ()> {
    let order = self.cmp(excl).ok_or(())?;

    match order {
      // There is no overlap between this range and the exclusion range. Simply
      // return this range.
      RangeOrdering::Less | RangeOrdering::Greater => {
        return Ok((Some(*self), None));
      }

      // This range is either exactly equal to or fully contained by the
      // exclusion range. This range is complete excluded.
      RangeOrdering::Equal | RangeOrdering::Subset => {
        return Ok((None, None));
      }

      _ => {}
    }

    let my_end = self.base + (self.size - 1);
    let excl_end = excl.base + (excl.size - 1);

    // The following two cases are mutually exclusive in the comparison result.
    //
    // self |---------------|
    // excl              |-----|
    //      |------------|
    //            a
    //
    // self    |---------------|
    // excl |-----|
    //            |------------|
    //                  b
    //
    // However, if the exclusion range is fully contained, the result is the
    // same as performing both of the above:
    //
    // self |---------------|
    // excl     |-----|
    //      |---|     |-----|
    //        a          b
    let a = match order {
      RangeOrdering::LessEqual | RangeOrdering::Superset => Some(Range {
        base: self.base,
        size: excl.base - self.base,
      }),

      _ => None,
    };

    let b = match order {
      RangeOrdering::GreaterEqual | RangeOrdering::Superset => Some(Range {
        base: excl_end + 1,
        size: my_end - excl_end,
      }),

      _ => None,
    };

    Ok((a, b))
  }
}
