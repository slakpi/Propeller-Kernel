//! Range Utilities

/// Range ordering.
pub enum RangeOrdering {
  /// The LHS is fully to the left of the RHS.
  Less,
  /// The LHS partially overlaps the beginning of the RHS.
  LessEqual,
  /// The LHS partially overlaps the end of the RHS.
  GreaterEqual,
  /// The LHS is fully to the right of the RHS.
  Greater,
  /// The two ranges are exactly equal.
  Equal,
  /// The LHS fully contains the RHS.
  Superset,
  /// The LHS is fully contained by the RHS.
  Subset,
}

/// A contiguous range of values in the interval `[base, base + size)`.
#[derive(Copy, Clone)]
pub struct Range<TagType>
where
  TagType: Copy,
{
  pub tag: TagType,
  pub base: usize,
  pub size: usize,
}

impl<TagType> Range<TagType>
where
  TagType: Copy,
{
  /// Compare two ranges.
  ///
  /// # Parameters
  ///
  /// * `rhs` - The range to compare against.
  ///
  /// # Returns
  ///
  /// A range ordering or None if the either range is invalid.
  pub fn cmp(&self, rhs: &Self) -> Option<RangeOrdering> {
    if self.size == 0 || rhs.size == 0 {
      return None;
    }

    let lhs_end = self.base + (self.size - 1);
    let rhs_end = rhs.base + (rhs.size - 1);

    if self.base == rhs.base && self.size == rhs.size {
      // lhs  |---------------|
      // rhs  |---------------|
      Some(RangeOrdering::Equal)
    } else if lhs_end < rhs.base {
      // lhs  |-----|
      // rhs         |---------------|
      Some(RangeOrdering::Less)
    } else if rhs_end < self.base {
      // lhs                   |-----|
      // rhs  |---------------|
      Some(RangeOrdering::Greater)
    } else if self.base <= rhs.base && lhs_end >= rhs_end {
      // lhs  |---------------|
      // rhs   |-----|
      Some(RangeOrdering::Superset)
    } else if rhs.base <= self.base && rhs_end >= lhs_end {
      // lhs           |-----|
      // rhs  |---------------|
      Some(RangeOrdering::Subset)
    } else if self.base < rhs.base {
      // lhs  |-----|
      // rhs     |---------------|
      Some(RangeOrdering::LessEqual)
    } else {
      // lhs               |-----|
      // rhs  |---------------|
      Some(RangeOrdering::GreaterEqual)
    }
  }

  /// Splits a range using an exclusion range.
  ///
  /// # Parameters
  ///
  /// * `excl` - The range to exclude.
  ///
  /// # Description
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
  /// If the range cannot be compared with the exclusion range, returns an
  /// Error.
  ///
  /// # Returns
  ///
  /// A tuple with the resulting range(s) of the split. See description.
  pub fn exclude(&self, excl: &Self) -> Result<(Option<Self>, Option<Self>), ()> {
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
      RangeOrdering::LessEqual | RangeOrdering::Superset => Some(Self {
        tag: self.tag,
        base: self.base,
        size: excl.base - self.base,
      }),

      _ => None,
    };

    let b = match order {
      RangeOrdering::GreaterEqual | RangeOrdering::Superset => Some(Self {
        tag: self.tag,
        base: excl_end + 1,
        size: my_end - excl_end,
      }),

      _ => None,
    };

    Ok((a, b))
  }

  /// Split a range at a specific point.
  ///
  /// # Parameters
  ///
  /// * `at` - The split point.
  ///
  /// # Description
  ///
  /// * If the split point is less than or equal to the range base, returns None
  ///   for the first element in the tuple and copy of the range in the second.
  ///
  /// * If the split point is greater than or equal to the open end of the
  ///   range, returns a copy of the range in the first element of the tuple and
  ///   None in the second.
  ///
  /// * If the split point is within the range, the first element of the tuple
  ///   is the low part of the range, `[base, at)`, and the second element is
  ///   the high part of the range, `[at, end)`.
  ///
  /// * If the size of the range is zero, returns an Error.
  ///
  /// # Returns
  ///
  /// A tuple with the resulting range(s) of the split. See description.
  pub fn split(&self, at: usize) -> Result<(Option<Self>, Option<Self>), ()> {
    if self.size == 0 {
      return Err(());
    }

    let my_end = self.base + (self.size - 1);

    if at <= self.base {
      return Ok((None, Some(*self)));
    } else if at > my_end {
      return Ok((Some(*self), None));
    }

    Ok((
      Some(Self {
        tag: self.tag,
        base: self.base,
        size: at - self.base,
      }),
      Some(Self {
        tag: self.tag,
        base: at,
        size: my_end - at + 1,
      }),
    ))
  }
}
