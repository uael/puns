//! The arguments of the process that runs the derive, as the system keeps them.
//!
//! A proc macro is a library with its own copy of `std`. Its `env::args_os` has the arguments where
//! the system keeps them for any code (`_NSGetArgv` on Apple systems, `GetCommandLineW` on
//! Windows), or where the C library hands them to the initializers of libraries (glibc). Elsewhere
//! it has none, since only the `std` of the executable, `rustc`, is given them. So the derive reads
//! them where the system keeps them: in `/proc/self/cmdline` on Linux (musl included), Android and
//! the other systems with a `/proc` of that format, and through `sysctl` on FreeBSD, DragonFly,
//! NetBSD and OpenBSD. On another system, it reads none.
//!
//! This file depends on `std` alone, so that it can be checked for the target of each system:
//! `rustc --edition 2024 --crate-type lib --emit metadata --target <target> src/args.rs`.

use std::ffi::OsString;

/// Returns the arguments of the process, the name of the program first, or `None` where the derive
/// cannot read them.
pub fn process() -> Option<Vec<OsString>> {
  let args = std::env::args_os().collect::<Vec<_>>();
  if !args.is_empty() {
    return Some(args);
  }
  system().filter(|args| !args.is_empty())
}

/// Returns the arguments of `bytes`, each ended by a NUL byte, or `None` if the last one is not.
#[cfg(all(unix, any(test, not(any(target_vendor = "apple", target_os = "openbsd")))))]
fn nul_ended(bytes: &[u8]) -> Option<Vec<OsString>> {
  use std::os::unix::ffi::OsStrExt;
  let body = bytes.strip_suffix(&[0])?;
  Some(
    body.split(|&byte| byte == 0).map(|arg| std::ffi::OsStr::from_bytes(arg).to_owned()).collect(),
  )
}

/// The arguments in `/proc/self/cmdline`, each ended by a NUL byte.
///
/// Before Linux 4.2, the file held at most one page of them. A length that is a multiple of 4096
/// bytes, the smallest page, may be such a cut, so it gives `None`: a cut may drop a value of
/// `-C metadata` given last.
#[cfg(all(
  unix,
  not(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
  ))
))]
fn system() -> Option<Vec<OsString>> {
  let bytes = std::fs::read("/proc/self/cmdline").ok()?;
  if bytes.len() % 4096 == 0 {
    return None;
  }
  nul_ended(&bytes)
}

/// The arguments that `sysctl` gives for the process (`kern.proc.args`), each ended by a NUL byte,
/// or, on OpenBSD, as a list of pointers into the buffer, ended by a null one, before them.
#[cfg(any(
  target_os = "freebsd",
  target_os = "dragonfly",
  target_os = "netbsd",
  target_os = "openbsd"
))]
fn system() -> Option<Vec<OsString>> {
  use std::ffi::{c_int, c_uint, c_void};
  unsafe extern "C" {
    fn sysctl(
      name: *const c_int,
      namelen: c_uint,
      oldp: *mut c_void,
      oldlenp: *mut usize,
      newp: *mut c_void,
      newlen: usize,
    ) -> c_int;
  }
  /// `CTL_KERN`, the first part of the name of the value: then `KERN_PROC` (14), `KERN_PROC_ARGS`
  /// (7) and the process on FreeBSD and DragonFly, or `KERN_PROC_ARGS` (48 on NetBSD, 55 on
  /// OpenBSD), the process and `KERN_PROC_ARGV` (1).
  const CTL_KERN: c_int = 1;
  /// `ENOMEM` on these systems: the buffer is too small.
  const ENOMEM: i32 = 12;
  let pid = c_int::try_from(std::process::id()).ok()?;
  #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
  let name = [CTL_KERN, 14, 7, pid];
  #[cfg(target_os = "netbsd")]
  let name = [CTL_KERN, 48, pid, 1];
  #[cfg(target_os = "openbsd")]
  let name = [CTL_KERN, 55, pid, 1];
  let mut buffer = vec![0u8; 1 << 16];
  let len = loop {
    let mut len = buffer.len();
    // SAFETY: `name` holds 4 integers, `buffer` is writable for `len` bytes, which `sysctl` writes
    // at most, and there is no new value.
    let status = unsafe {
      sysctl(name.as_ptr(), 4, buffer.as_mut_ptr().cast(), &raw mut len, std::ptr::null_mut(), 0)
    };
    if status == 0 {
      break len;
    }
    if std::io::Error::last_os_error().raw_os_error() != Some(ENOMEM) || buffer.len() >= 1 << 26 {
      return None;
    }
    buffer.resize(buffer.len() * 2, 0);
  };
  let bytes = buffer.get(..len)?;
  #[cfg(not(target_os = "openbsd"))]
  return nul_ended(bytes);
  #[cfg(target_os = "openbsd")]
  return pointed(bytes, buffer.as_ptr().addr());
}

/// Returns the arguments of `bytes`, a buffer at the address `at` that starts with pointers to
/// them, ended by a null one, each argument ended by a NUL byte, as OpenBSD's `sysctl` writes them.
/// Reads the pointers as offsets into `bytes`, and nothing outside it.
#[cfg(all(unix, any(test, target_os = "openbsd")))]
fn pointed(bytes: &[u8], at: usize) -> Option<Vec<OsString>> {
  use std::os::unix::ffi::OsStrExt;
  const WORD: usize = size_of::<usize>();
  let mut args = vec![];
  for word in bytes.chunks(WORD) {
    let pointer = usize::from_ne_bytes(word.try_into().ok()?);
    if pointer == 0 {
      return Some(args);
    }
    let arg = bytes.get(pointer.checked_sub(at)?..)?;
    let end = arg.iter().position(|&byte| byte == 0)?;
    args.push(std::ffi::OsStr::from_bytes(&arg[..end]).to_owned());
  }
  None
}

/// On Apple systems, `std` has the arguments, and on another system, the derive reads none.
#[cfg(any(not(unix), target_vendor = "apple"))]
const fn system() -> Option<Vec<OsString>> {
  None
}

#[cfg(all(test, unix))]
mod tests {
  use std::ffi::OsString;

  use super::{nul_ended, pointed};

  #[test]
  fn arguments_ended_by_nul_bytes() {
    let args = nul_ended(b"rustc\0--crate-name\0x\0\0").unwrap();
    assert_eq!(args, ["rustc", "--crate-name", "x", ""].map(OsString::from));
    assert_eq!(nul_ended(b"\0"), Some(vec![OsString::new()]));
    assert_eq!(nul_ended(b""), None);
    assert_eq!(nul_ended(b"rustc\0--crate-na"), None);
  }

  #[test]
  fn arguments_behind_pointers_into_the_buffer() {
    const WORD: usize = size_of::<usize>();
    let at = 0x1000;
    let mut bytes = vec![];
    for pointer in [at + 3 * WORD, at + 3 * WORD + 6, 0] {
      bytes.extend(usize::to_ne_bytes(pointer));
    }
    bytes.extend(b"rustc\0-v\0");
    assert_eq!(pointed(&bytes, at), Some(["rustc", "-v"].map(OsString::from).to_vec()));
    // A pointer out of the buffer, an argument without its end, or no null pointer.
    assert_eq!(pointed(&bytes, at + (1 << 20)), None);
    assert_eq!(pointed(&bytes[..bytes.len() - 1], at), None);
    assert_eq!(pointed(&bytes[..WORD], at), None);
  }

  /// Checks that the arguments that the system keeps for the process are those of `std`, which a
  /// test, an executable, has everywhere.
  #[cfg(not(target_vendor = "apple"))]
  #[test]
  fn the_system_keeps_the_arguments_of_std() {
    assert_eq!(super::system(), Some(std::env::args_os().collect()));
  }
}
