//! Hash Map Utilities

use core::cmp::Eq;
use core::convert::TryFrom;
use core::hash::{BuildHasher, Hash, Hasher};

/// Hash map implementation with fixed-size storage. The hash table should be a
/// prime number larger than the expected number of items.
///
///     TODO: Implement rehashing? This map should not be used in situations
///           that thrash the contents until the map has the ability optimize
///           by rehashing to reduce probing.
pub struct HashMap<K, V, S, const N: usize>
where
  K: Eq + Hash,
  S: BuildHasher,
{
  map: [Option<(K, V)>; N],
  hasher_factory: S,
}

impl<K, V, S, const N: usize> HashMap<K, V, S, N>
where
  K: Eq + Hash,
  S: BuildHasher,
{
  const TABLE_INITIALIZER: Option<(K, V)> = None;

  /// Calculate an index into the hash table using quadratic probing.
  ///
  /// # Parameters
  ///
  /// * `h` - The hash value of the key.
  /// * `i` - Probe iteration.
  ///
  /// # Description
  ///
  /// Indices are calculated as follows:
  ///
  ///     q = ( h + i^2 ) % m
  ///
  /// Where `m` is the size of the hash table.
  ///
  /// # Returns
  ///
  /// The probe index.
  fn make_probe_index(h: u64, i: usize, m: usize) -> usize {
    // Perform the quadratic probe calculation in u64. In general, the size of
    // the hash table array, N, should be << u32::MAX. If not, we will likely
    // have a much bigger issues. So, the usize -> u64 conversions should never
    // panic in unwrap. Likewise, the conversion from u64 -> usize should never
    // panic in unwrap since 32-bit bit is the minimum platform usize we
    // support. If these assumptions do not hold, the panic will let us know...
    // assuming we can even load the kernel into memory.
    let i = u64::try_from(i).unwrap();
    let m = u64::try_from(m).unwrap();
    let idx = (h + (i * i)) % m;
    usize::try_from(idx).unwrap()
  }

  /// Construct a new HashMap with a Hasher factory.
  ///
  /// # Parameters
  ///
  /// `builder` - A factory to use for constructing hasher objects.
  ///
  /// # Returns
  ///
  /// A new, empty HashMap.
  pub fn with_hasher_factory(hasher_factory: S) -> Self {
    HashMap {
      map: [Self::TABLE_INITIALIZER; N],
      hasher_factory,
    }
  }

  /// Insert a new (key, value) pair.
  ///
  /// # Parameters
  ///
  /// * `key` - The indexing key.
  /// * `value` - The associated value.
  ///
  /// # Returns
  ///
  /// True if able to insert the pair, false otherwise.
  pub fn insert(&mut self, key: K, value: V) -> bool {
    let h = self.hash_key(&key);

    for i in 0..N {
      let idx = Self::make_probe_index(h, i, N);
      if let Some(p) = &mut self.map[idx] {
        if p.0 == key {
          p.1 = value;
          return true;
        }
      } else {
        self.map[idx] = Some((key, value));
        return true;
      }
    }

    false
  }

  /// Find a value given a key.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to search for.
  ///
  /// # Returns
  ///
  /// A reference to the value if it exists, or None.
  pub fn find(&self, key: K) -> Option<&'_ V> {
    let h = self.hash_key(&key);

    for i in 0..N {
      let idx = Self::make_probe_index(h, i, N);
      if let Some(p) = &self.map[idx] {
        if p.0 == key {
          return Some(&p.1);
        }
      }
    }

    None
  }

  /// Erase a (key, value) pair from the map.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to remove.
  ///
  /// # Returns
  ///
  /// True if the pair existed, false otherwise.
  pub fn erase(&mut self, key: K) -> bool {
    let h = self.hash_key(&key);

    for i in 0..N {
      let idx = Self::make_probe_index(h, i, N);
      if let Some(p) = &self.map[idx] {
        if p.0 == key {
          self.map[idx] = None;
          return true;
        }
      }
    }

    false
  }

  /// Hash a key with the provided hasher factory.
  ///
  /// # Parameters
  ///
  /// * `key` - The key to hash.
  ///
  /// # Returns
  ///
  /// The key hash.
  fn hash_key(&self, key: &K) -> u64 {
    let mut hasher = self.hasher_factory.build_hasher();
    key.hash(&mut hasher);
    hasher.finish()
  }
}
