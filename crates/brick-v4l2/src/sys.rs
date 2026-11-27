//! The parts of the V4L2 kernel interface (`linux/videodev2.h`) Brick uses.
//!
//! Every struct here mirrors the kernel's struct of the same name field for
//! field, with `#[repr(C)]`, so it has the same layout. Unions in the kernel
//! structs are written as structs holding the one member Brick uses, padded
//! and aligned to the union's size, so no union field is ever read. The sizes
//! are checked against the kernel's at compile time on 64 bit targets.

use std::fs::File;
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::fd::AsRawFd;

pub const BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
pub const MEMORY_MMAP: u32 = 1;
pub const FIELD_NONE: u32 = 1;
pub const FRMSIZE_TYPE_DISCRETE: u32 = 1;

pub const CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
pub const CAP_STREAMING: u32 = 0x0400_0000;
pub const CAP_DEVICE_CAPS: u32 = 0x8000_0000;

pub const BUF_FLAG_ERROR: u32 = 0x0040;
pub const CTRL_FLAG_DISABLED: u32 = 0x0001;
pub const CTRL_FLAG_INACTIVE: u32 = 0x0010;

const CID_BASE: u32 = 0x0098_0900;
const CID_CAMERA_CLASS_BASE: u32 = 0x009a_0900;
pub const CID_AUTO_WHITE_BALANCE: u32 = CID_BASE + 12;
pub const CID_GAIN: u32 = CID_BASE + 19;
pub const CID_WHITE_BALANCE_TEMPERATURE: u32 = CID_BASE + 26;
/// A menu: 0 auto, 1 manual, 2 shutter priority, 3 aperture priority.
pub const CID_EXPOSURE_AUTO: u32 = CID_CAMERA_CLASS_BASE + 1;
/// Exposure time in units of 100 µs.
pub const CID_EXPOSURE_ABSOLUTE: u32 = CID_CAMERA_CLASS_BASE + 2;
/// Whether auto exposure may lower the frame rate.
pub const CID_EXPOSURE_AUTO_PRIORITY: u32 = CID_CAMERA_CLASS_BASE + 3;
pub const CID_FOCUS_ABSOLUTE: u32 = CID_CAMERA_CLASS_BASE + 10;
pub const CID_FOCUS_AUTO: u32 = CID_CAMERA_CLASS_BASE + 12;
/// The `CID_EXPOSURE_AUTO` value for manual exposure.
pub const EXPOSURE_MANUAL: i32 = 1;

/// `struct v4l2_capability`
#[repr(C)]
pub struct Capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

/// `struct v4l2_fmtdesc`
#[repr(C)]
pub struct FmtDesc {
    pub index: u32,
    pub type_: u32,
    pub flags: u32,
    pub description: [u8; 32],
    pub pixelformat: u32,
    pub mbus_code: u32,
    pub reserved: [u32; 3],
}

/// `struct v4l2_pix_format`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PixFormat {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub colorspace: u32,
    pub priv_: u32,
    pub flags: u32,
    pub ycbcr_enc: u32,
    pub quantization: u32,
    pub xfer_func: u32,
}

/// The `fmt` union of `struct v4l2_format`: 200 bytes, aligned like a
/// pointer because some members hold pointers.
#[repr(C)]
pub struct FormatData {
    pub pix: PixFormat,
    pub rest: [u8; 200 - size_of::<PixFormat>()],
    pub align: [libc::c_ulong; 0],
}

/// `struct v4l2_format`
#[repr(C)]
pub struct Format {
    pub type_: u32,
    pub fmt: FormatData,
}

/// `struct v4l2_requestbuffers`
#[repr(C)]
pub struct RequestBuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub flags: u8,
    pub reserved: [u8; 3],
}

/// The `m` union of `struct v4l2_buffer`, sized and aligned like its largest
/// members, an `unsigned long` and a pointer. Brick only uses `offset`, which
/// is at the start of the union.
#[repr(C)]
pub struct BufferM {
    pub offset: u32,
    pub rest: [u8; size_of::<libc::c_ulong>() - 4],
    pub align: [libc::c_ulong; 0],
}

/// `struct v4l2_buffer`
#[repr(C)]
pub struct Buffer {
    pub index: u32,
    pub type_: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub field: u32,
    pub timestamp: libc::timeval,
    /// `struct v4l2_timecode`, unused.
    pub timecode: [u32; 4],
    pub sequence: u32,
    pub memory: u32,
    pub m: BufferM,
    pub length: u32,
    pub reserved2: u32,
    /// The `request_fd` union.
    pub request_fd: i32,
}

impl Buffer {
    /// A memory mapped capture buffer with this index.
    pub fn new(index: u32) -> Self {
        Buffer {
            index,
            type_: BUF_TYPE_VIDEO_CAPTURE,
            memory: MEMORY_MMAP,
            ..Default::default()
        }
    }
}

/// `struct v4l2_control`
#[repr(C)]
pub struct Control {
    pub id: u32,
    pub value: i32,
}

/// `struct v4l2_queryctrl`
#[repr(C)]
pub struct QueryCtrl {
    pub id: u32,
    pub type_: u32,
    pub name: [u8; 32],
    pub minimum: i32,
    pub maximum: i32,
    pub step: i32,
    pub default_value: i32,
    pub flags: u32,
    pub reserved: [u32; 2],
}

/// `struct v4l2_frmsizeenum`. `data` is the union of the discrete size
/// (width, height) and the stepwise range (min, max, and step of the width,
/// then of the height).
#[repr(C)]
pub struct FrmSizeEnum {
    pub index: u32,
    pub pixel_format: u32,
    pub type_: u32,
    pub data: [u32; 6],
    pub reserved: [u32; 2],
}

/// Implements `Default` as all zero bytes, which is how V4L2 expects
/// requests to be initialised.
macro_rules! zeroed_default {
    ($($ty:ty),*) => {$(
        impl Default for $ty {
            fn default() -> Self {
                // SAFETY: the struct only holds integers, arrays of integers,
                // and `libc::timeval` (also integers), for which all zero
                // bytes is a valid value.
                unsafe { std::mem::zeroed() }
            }
        }
    )*};
}

zeroed_default!(
    Capability,
    FmtDesc,
    Format,
    RequestBuffers,
    Buffer,
    QueryCtrl,
    FrmSizeEnum
);

// The kernel's sizes, from the request codes in `linux/videodev2.h`.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<Capability>() == 104);
    assert!(size_of::<FmtDesc>() == 64);
    assert!(size_of::<Format>() == 208);
    assert!(size_of::<RequestBuffers>() == 20);
    assert!(size_of::<Buffer>() == 88);
    assert!(size_of::<Control>() == 8);
    assert!(size_of::<QueryCtrl>() == 68);
    assert!(size_of::<FrmSizeEnum>() == 44);
};

/// An ioctl request code for an argument of type `T`. Codes can only be
/// made in this module, from the kernel's request numbers and the size of
/// `T`, so a code always matches its argument type.
pub struct Code<T> {
    value: u32,
    argument: PhantomData<fn(&mut T)>,
}

impl<T> Clone for Code<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Code<T> {}

const WRITE: u32 = 1;
const READ: u32 = 2;

/// `_IOC(dir, 'V', nr, sizeof(T))`
const fn code<T>(dir: u32, nr: u32) -> Code<T> {
    Code {
        value: (dir << 30) | ((size_of::<T>() as u32) << 16) | ((b'V' as u32) << 8) | nr,
        argument: PhantomData,
    }
}

pub const QUERYCAP: Code<Capability> = code(READ, 0);
pub const ENUM_FMT: Code<FmtDesc> = code(READ | WRITE, 2);
pub const S_FMT: Code<Format> = code(READ | WRITE, 5);
pub const REQBUFS: Code<RequestBuffers> = code(READ | WRITE, 8);
pub const QUERYBUF: Code<Buffer> = code(READ | WRITE, 9);
pub const QBUF: Code<Buffer> = code(READ | WRITE, 15);
pub const DQBUF: Code<Buffer> = code(READ | WRITE, 17);
pub const STREAMON: Code<libc::c_int> = code(WRITE, 18);
pub const STREAMOFF: Code<libc::c_int> = code(WRITE, 19);
pub const G_CTRL: Code<Control> = code(READ | WRITE, 27);
pub const S_CTRL: Code<Control> = code(READ | WRITE, 28);
pub const QUERYCTRL: Code<QueryCtrl> = code(READ | WRITE, 36);
pub const ENUM_FRAMESIZES: Code<FrmSizeEnum> = code(READ | WRITE, 74);

/// Runs an ioctl on a V4L2 device, retrying if interrupted.
pub fn ioctl<T>(file: &File, code: Code<T>, arg: &mut T) -> io::Result<()> {
    loop {
        // SAFETY: `code` was built from the kernel's request number and the
        // size of `T`, and `T` has the kernel's layout for that request (see
        // the module docs), so the kernel reads and writes only within the
        // `size_of::<T>()` bytes `arg` points to. `arg` is a unique, live
        // reference for the duration of the call.
        let result = unsafe { libc::ioctl(file.as_raw_fd(), code.value as _, arg as *mut T) };
        if result != -1 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;

    #[test]
    fn request_codes_match_the_kernel() {
        assert_eq!(QUERYCAP.value, 0x8068_5600);
        assert_eq!(ENUM_FMT.value, 0xc040_5602);
        assert_eq!(S_FMT.value, 0xc0d0_5605);
        assert_eq!(REQBUFS.value, 0xc014_5608);
        assert_eq!(QUERYBUF.value, 0xc058_5609);
        assert_eq!(QBUF.value, 0xc058_560f);
        assert_eq!(DQBUF.value, 0xc058_5611);
        assert_eq!(STREAMON.value, 0x4004_5612);
        assert_eq!(STREAMOFF.value, 0x4004_5613);
        assert_eq!(G_CTRL.value, 0xc008_561b);
        assert_eq!(S_CTRL.value, 0xc008_561c);
        assert_eq!(QUERYCTRL.value, 0xc044_5624);
        assert_eq!(ENUM_FRAMESIZES.value, 0xc02c_564a);
    }
}
