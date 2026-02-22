//! Kernel Printing Utilities

use core::{cmp, fmt};

/// A thin wrapper around a buffer to track writes during string formatting.
pub struct WriteBuffer<'buffer> {
  buf: &'buffer mut [u8],
  offset: usize,
}

impl<'buffer> WriteBuffer<'buffer> {
  /// Create a new wrapper around the specified buffer.
  ///
  /// # Parameters
  ///
  /// * `buf` - The buffer to use for formatting.
  ///
  /// # Returns
  ///
  /// A new write buffer.
  pub fn new(buf: &'buffer mut [u8]) -> Self {
    WriteBuffer { buf, offset: 0 }
  }

  /// Access the bytes of the buffer.
  ///
  /// # Returns
  ///
  /// A slice containing only the bytes written to the buffer.
  pub fn as_bytes(&self) -> &[u8] {
    &self.buf[..self.offset]
  }
}

impl<'buffer> fmt::Write for WriteBuffer<'buffer> {
  /// See `fmt::Write::write_str()`.
  ///
  /// # Parameters
  ///
  /// * `s` - The string to write.
  ///
  /// # Returns
  ///
  /// Unconditionally returns Ok. The string will be truncated if it does not
  /// fit in the buffer.
  fn write_str(&mut self, s: &str) -> fmt::Result {
    let bytes = s.as_bytes();
    let dest_len = self.buf.len() - self.offset;
    let copy_len = cmp::min(bytes.len(), dest_len);

    let dest = &mut self.buf[self.offset..];
    let dest = &mut dest[..copy_len];
    dest.copy_from_slice(&bytes[..copy_len]);
    self.offset += copy_len;

    Ok(())
  }
}
