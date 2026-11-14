//! Randomness from the operating system (`/dev/urandom`), with no new dependency.

use std::io::Read;

/// `n` random bytes from the OS.
///
/// # Errors
/// The IO error when the OS source cannot be read.
pub fn os_random(n: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf)
}
