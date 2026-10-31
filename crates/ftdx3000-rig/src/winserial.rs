//! The radio's CAT port on Windows, opened for overlapped I/O.
//!
//! `serialport` opens COM ports for blocking I/O, and its `try_clone` shares
//! the one file object. Windows runs blocking requests on a file object one
//! at a time, so every write to the radio waited behind the reader thread's
//! pending read, up to its 50 ms timeout: frequency changes crawled and the
//! poll queue never drained. Overlapped reads and writes run side by side,
//! as reads and writes do on Linux and macOS.
//!
//! The port is set up the way `serialport` sets it up (same DCB fields and
//! timeouts), so the radio sees no difference.

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::sync::Arc;

use crate::transport::FlowControl;

type Handle = *mut c_void;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const ERROR_IO_PENDING: i32 = 997;
const SETRTS: u32 = 3;
const CLRRTS: u32 = 4;
const CLRDTR: u32 = 6;
const PURGE_TXCLEAR: u32 = 0x4;
const PURGE_RXCLEAR: u32 = 0x8;
const NOPARITY: u8 = 0;
const ONESTOPBIT: u8 = 0;
const TWOSTOPBITS: u8 = 2;
/// Read and write timeout, as `serialport` was given it.
const TIMEOUT_MS: u32 = 50;

#[repr(C)]
#[derive(Default)]
struct Dcb {
    length: u32,
    baud_rate: u32,
    /// fBinary, fParity, fOutxCtsFlow, ... (see `set_bits`).
    flags: u32,
    reserved: u16,
    xon_lim: u16,
    xoff_lim: u16,
    byte_size: u8,
    parity: u8,
    stop_bits: u8,
    xon_char: i8,
    xoff_char: i8,
    error_char: i8,
    eof_char: i8,
    evt_char: i8,
    reserved1: u16,
}

impl Dcb {
    fn set_bits(&mut self, shift: u32, width: u32, value: u32) {
        let mask = ((1 << width) - 1) << shift;
        self.flags = (self.flags & !mask) | ((value << shift) & mask);
    }
}

// Bit positions in `Dcb::flags`.
const F_BINARY: u32 = 0;
const F_PARITY: u32 = 1;
const F_OUTX_CTS_FLOW: u32 = 2;
const F_OUTX_DSR_FLOW: u32 = 3;
const F_DTR_CONTROL: u32 = 4; // 2 bits
const F_DSR_SENSITIVITY: u32 = 6;
const F_OUTX: u32 = 8;
const F_INX: u32 = 9;
const F_ERROR_CHAR: u32 = 10;
const F_NULL: u32 = 11;
const F_RTS_CONTROL: u32 = 12; // 2 bits
const F_ABORT_ON_ERROR: u32 = 14;
const RTS_CONTROL_DISABLE: u32 = 0;
const RTS_CONTROL_ENABLE: u32 = 1;

#[repr(C)]
struct CommTimeouts {
    read_interval: u32,
    read_total_multiplier: u32,
    read_total_constant: u32,
    write_total_multiplier: u32,
    write_total_constant: u32,
}

#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: Handle,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(name: *const u16, access: u32, share: u32, security: *mut c_void, disposition: u32, flags: u32, template: Handle) -> Handle;
    fn CloseHandle(h: Handle) -> i32;
    fn GetCommState(h: Handle, dcb: *mut Dcb) -> i32;
    fn SetCommState(h: Handle, dcb: *const Dcb) -> i32;
    fn SetCommTimeouts(h: Handle, t: *const CommTimeouts) -> i32;
    fn EscapeCommFunction(h: Handle, func: u32) -> i32;
    fn PurgeComm(h: Handle, flags: u32) -> i32;
    fn ReadFile(h: Handle, buf: *mut u8, len: u32, done: *mut u32, ov: *mut Overlapped) -> i32;
    fn WriteFile(h: Handle, buf: *const u8, len: u32, done: *mut u32, ov: *mut Overlapped) -> i32;
    fn GetOverlappedResult(h: Handle, ov: *const Overlapped, done: *mut u32, wait: i32) -> i32;
    fn FlushFileBuffers(h: Handle) -> i32;
    fn CreateEventW(security: *mut c_void, manual_reset: i32, initial: i32, name: *const u16) -> Handle;
}

/// A handle closed on drop.
struct Owned(Handle);

// Windows handles may be used from any thread.
unsafe impl Send for Owned {}
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn check(ok: i32) -> io::Result<()> {
    if ok != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// One half of the port: the shared handle and its own completion event.
pub struct Half {
    port: Arc<Owned>,
    event: Owned,
}

impl Half {
    fn new(port: Arc<Owned>) -> io::Result<Half> {
        let event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Half { port, event: Owned(event) })
    }

    /// Start an overlapped request and wait for it to finish; the port's
    /// timeouts end it.
    fn run(&mut self, start: impl FnOnce(Handle, *mut Overlapped) -> i32) -> io::Result<usize> {
        let mut ov = Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event: self.event.0 };
        if start(self.port.0, &mut ov) == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(ERROR_IO_PENDING) {
                return Err(e);
            }
        }
        let mut done = 0u32;
        check(unsafe { GetOverlappedResult(self.port.0, &ov, &mut done, 1) })?;
        Ok(done as usize)
    }
}

impl Read for Half {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_mut_ptr();
        match self.run(|h, ov| unsafe { ReadFile(h, ptr, len, std::ptr::null_mut(), ov) })? {
            0 => Err(io::Error::new(io::ErrorKind::TimedOut, "Operation timed out")),
            n => Ok(n),
        }
    }
}

impl Write for Half {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_ptr();
        self.run(|h, ov| unsafe { WriteFile(h, ptr, len, std::ptr::null_mut(), ov) })
    }

    fn flush(&mut self) -> io::Result<()> {
        check(unsafe { FlushFileBuffers(self.port.0) })
    }
}

/// Open `path` ("COM4") exclusively: a reader and a writer half.
pub fn open(path: &str, baud: u32, stop_bits: u8, flow: FlowControl) -> io::Result<(Half, Half)> {
    let mut name: Vec<u16> = Vec::new();
    if !path.starts_with('\\') {
        name.extend(r"\\.\".encode_utf16());
    }
    name.extend(path.encode_utf16());
    name.push(0);
    let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, std::ptr::null_mut(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED, std::ptr::null_mut()) };
    if h as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let port = Arc::new(Owned(h));

    let mut dcb = Dcb { length: std::mem::size_of::<Dcb>() as u32, ..Default::default() };
    check(unsafe { GetCommState(h, &mut dcb) })?;
    dcb.xon_char = 17;
    dcb.xoff_char = 19;
    dcb.error_char = 0;
    dcb.eof_char = 26;
    dcb.baud_rate = baud;
    dcb.byte_size = 8;
    dcb.parity = NOPARITY;
    dcb.stop_bits = if stop_bits == 1 { ONESTOPBIT } else { TWOSTOPBITS };
    let hardware = flow == FlowControl::Hardware;
    for (bit, on) in [
        (F_BINARY, true),
        (F_PARITY, false),
        (F_OUTX_CTS_FLOW, hardware),
        (F_OUTX_DSR_FLOW, false),
        (F_DSR_SENSITIVITY, false),
        (F_OUTX, false),
        (F_INX, false),
        (F_ERROR_CHAR, false),
        (F_NULL, false),
        (F_ABORT_ON_ERROR, false),
    ] {
        dcb.set_bits(bit, 1, on as u32);
    }
    dcb.set_bits(F_DTR_CONTROL, 2, 0); // DTR_CONTROL_DISABLE
    dcb.set_bits(F_RTS_CONTROL, 2, if hardware { RTS_CONTROL_ENABLE } else { RTS_CONTROL_DISABLE });
    check(unsafe { SetCommState(h, &dcb) })?;

    // Reads return as soon as a byte arrives, or empty after the timeout.
    let timeouts = CommTimeouts {
        read_interval: u32::MAX,
        read_total_multiplier: u32::MAX,
        read_total_constant: TIMEOUT_MS,
        write_total_multiplier: 0,
        write_total_constant: TIMEOUT_MS,
    };
    check(unsafe { SetCommTimeouts(h, &timeouts) })?;
    match flow {
        FlowControl::None => {
            unsafe { EscapeCommFunction(h, SETRTS) };
        }
        FlowControl::Quiet => {
            unsafe { EscapeCommFunction(h, CLRRTS) };
            unsafe { EscapeCommFunction(h, CLRDTR) };
        }
        FlowControl::Hardware => {}
    }
    unsafe { PurgeComm(h, PURGE_RXCLEAR | PURGE_TXCLEAR) };
    Ok((Half::new(port.clone())?, Half::new(port)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layouts_match_windows() {
        assert_eq!(std::mem::size_of::<Dcb>(), 28);
        assert_eq!(std::mem::size_of::<CommTimeouts>(), 20);
        assert_eq!(std::mem::size_of::<Overlapped>(), if cfg!(target_pointer_width = "64") { 32 } else { 20 });
    }

    #[test]
    fn dcb_bits() {
        let mut d = Dcb::default();
        d.set_bits(F_BINARY, 1, 1);
        d.set_bits(F_OUTX_CTS_FLOW, 1, 1);
        d.set_bits(F_RTS_CONTROL, 2, RTS_CONTROL_ENABLE);
        assert_eq!(d.flags, 0b1 | 0b100 | (1 << 12));
        d.set_bits(F_OUTX_CTS_FLOW, 1, 0);
        d.set_bits(F_RTS_CONTROL, 2, RTS_CONTROL_DISABLE);
        assert_eq!(d.flags, 1);
    }
}
