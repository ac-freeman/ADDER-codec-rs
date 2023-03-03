#![warn(missing_docs)]

//! # adder-codec_old-core
//!
//! The core types and utilities for encoding and decoding ADΔER events

/// Expose public API for encoding and decoding
pub mod codec;
mod codec_old;

pub use bitstream_io;
use bitstream_io::{BigEndian, BitReader};
use std::cmp::{max, min};
use std::fs::File;
use std::io::BufReader;
use std::ops::Add;

use thiserror::Error;

/// Error type for the `PlaneSize` struct
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum PlaneError {
    #[error(
        "plane dimensions invalid. All must be positive. Found {width:?}, {height:?}, {channels:?}"
    )]
    InvalidPlane {
        width: u16,
        height: u16,
        channels: u8,
    },
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum SourceCamera {
    #[default]
    FramedU8,
    FramedU16,
    FramedU32,
    FramedU64,
    FramedF32,
    FramedF64,
    Dvs,
    DavisU8,
    Atis,
    Asint,
}

use crate::codec::compressed::blocks::{DeltaTResidual, EventResidual};
use crate::codec::compressed::stream::CompressedInput;
use crate::codec::decoder::Decoder;
use crate::codec::raw::stream::RawInput;
use crate::codec::{CodecError, ReadCompression};
use crate::codec_old::compressed::compression::DResidual;
use serde::{Deserialize, Serialize};

/// The type of time used in the ADΔER representation
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum TimeMode {
    /// The time is the delta time from the previous event
    DeltaT,

    /// The time is the absolute time from the start of the recording
    #[default]
    AbsoluteT,

    /// TODO
    Mixed,
}

/// The size of the image plane in pixels
#[derive(Clone, Copy)]
pub struct PlaneSize {
    width: u16,
    height: u16,
    channels: u8,
}

impl Default for PlaneSize {
    fn default() -> Self {
        PlaneSize {
            width: 1,
            height: 1,
            channels: 1,
        }
    }
}

impl PlaneSize {
    /// Create a new `PlaneSize` with the given width, height, and channels
    pub fn new(width: u16, height: u16, channels: u8) -> Result<Self, PlaneError> {
        if width == 0 || height == 0 || channels == 0 {
            return Err(PlaneError::InvalidPlane {
                width,
                height,
                channels,
            });
        }
        Ok(Self {
            width,
            height,
            channels,
        })
    }
    /// The width, shorthand for `self.width`
    pub fn w(&self) -> u16 {
        self.width
    }

    /// The height, shorthand for `self.height`
    pub fn w_usize(&self) -> usize {
        self.width as usize
    }

    /// The height, shorthand for `self.height`
    pub fn h(&self) -> u16 {
        self.height
    }

    /// The height, shorthand for `self.height`
    pub fn h_usize(&self) -> usize {
        self.height as usize
    }

    /// The number of channels, shorthand for `self.channels`
    pub fn c(&self) -> u8 {
        self.channels
    }

    /// The number of channels, shorthand for `self.channels`
    pub fn c_usize(&self) -> usize {
        self.channels as usize
    }

    /// The total number of 2D pixels in the image plane, across the height and width dimension
    pub fn area_wh(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// The total number of 2D pixels in the image plane, across the width and channel dimension
    pub fn area_wc(&self) -> usize {
        self.width as usize * self.channels as usize
    }

    /// The total number of 2D pixels in the image plane, across the height and channel dimension
    pub fn area_hc(&self) -> usize {
        self.height as usize * self.channels as usize
    }

    /// The total number of 3D pixels in the image plane (2D pixels * color depth)
    pub fn volume(&self) -> usize {
        self.area_wh() * self.channels as usize
    }
}

/// Decimation value; a pixel's sensitivity.
pub type D = u8;

/// The maximum possible [`D`] value
pub const D_MAX: D = 127;

/// Special symbol signifying no information (filler dt)
pub const D_EMPTY: D = 255;

/// Special symbol signifying no information (filler dt)
pub const D_ZERO_INTEGRATION: D = 254;

/// Special symbol signifying no [`Event`] exists
// pub const D_NO_EVENT: D = 253;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    FramePerfect,
    Continuous,
}
/// Precision for maximum intensity representable with allowed [`D`] values
pub type UDshift = u128;

/// Array for computing the intensity to integrate for a given [`D`] value
pub const D_SHIFT: [UDshift; 128] = [
    1 << 0,
    1 << 1,
    1 << 2,
    1 << 3,
    1 << 4,
    1 << 5,
    1 << 6,
    1 << 7,
    1 << 8,
    1 << 9,
    1 << 10,
    1 << 11,
    1 << 12,
    1 << 13,
    1 << 14,
    1 << 15,
    1 << 16,
    1 << 17,
    1 << 18,
    1 << 19,
    1 << 20,
    1 << 21,
    1 << 22,
    1 << 23,
    1 << 24,
    1 << 25,
    1 << 26,
    1 << 27,
    1 << 28,
    1 << 29,
    1 << 30,
    1 << 31,
    1 << 32,
    1 << 33,
    1 << 34,
    1 << 35,
    1 << 36,
    1 << 37,
    1 << 38,
    1 << 39,
    1 << 40,
    1 << 41,
    1 << 42,
    1 << 43,
    1 << 44,
    1 << 45,
    1 << 46,
    1 << 47,
    1 << 48,
    1 << 49,
    1 << 50,
    1 << 51,
    1 << 52,
    1 << 53,
    1 << 54,
    1 << 55,
    1 << 56,
    1 << 57,
    1 << 58,
    1 << 59,
    1 << 60,
    1 << 61,
    1 << 62,
    1 << 63,
    1 << 64,
    1 << 65,
    1 << 66,
    1 << 67,
    1 << 68,
    1 << 69,
    1 << 70,
    1 << 71,
    1 << 72,
    1 << 73,
    1 << 74,
    1 << 75,
    1 << 76,
    1 << 77,
    1 << 78,
    1 << 79,
    1 << 80,
    1 << 81,
    1 << 82,
    1 << 83,
    1 << 84,
    1 << 85,
    1 << 86,
    1 << 87,
    1 << 88,
    1 << 89,
    1 << 90,
    1 << 91,
    1 << 92,
    1 << 93,
    1 << 94,
    1 << 95,
    1 << 96,
    1 << 97,
    1 << 98,
    1 << 99,
    1 << 100,
    1 << 101,
    1 << 102,
    1 << 103,
    1 << 104,
    1 << 105,
    1 << 106,
    1 << 107,
    1 << 108,
    1 << 109,
    1 << 110,
    1 << 111,
    1 << 112,
    1 << 113,
    1 << 114,
    1 << 115,
    1 << 116,
    1 << 117,
    1 << 118,
    1 << 119,
    1 << 120,
    1 << 121,
    1 << 122,
    1 << 123,
    1 << 124,
    1 << 125,
    1 << 126,
    1 << 127,
];

/// The maximum intensity representation for input data. Currently 255 for 8-bit framed input.
pub const MAX_INTENSITY: f32 = 255.0; // TODO: make variable, dependent on input bit depth

/// The default [`D`] value for every pixel at the beginning of transcode
pub const D_START: D = 7;

/// Number of ticks elapsed since a given pixel last fired an [`Event`]
pub type DeltaT = u32;

pub type AbsoluteT = u32;

/// Large count of ticks (e.g., for tracking the running timestamp of a sequence of [Events](Event)
pub type BigT = u64;

/// Measure of an amount of light intensity
pub type Intensity = f64;

/// Pixel x- or y- coordinate address in the ADΔER model
pub type PixelAddress = u16;

/// Special pixel address when signifying the end of a sequence of [Events](Event)
pub const EOF_PX_ADDRESS: PixelAddress = u16::MAX;

/// Pixel channel address in the ADΔER model
#[repr(packed)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Coord {
    /// Pixel x-coordinate
    pub x: PixelAddress,

    /// Pixel y-coordinate
    pub y: PixelAddress,

    /// Pixel channel, if present
    pub c: Option<u8>,
}

impl Default for Coord {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            c: Some(0),
        }
    }
}

impl Coord {
    /// Creates a new coordinate with the given x, y, and channel
    pub fn new(x: PixelAddress, y: PixelAddress, c: Option<u8>) -> Self {
        Self { x, y, c }
    }

    /// Creates a new 2D coordinate
    pub fn new_2d(x: PixelAddress, y: PixelAddress) -> Self {
        Self { x, y, c: None }
    }

    /// Creates a new 3D coordinate with the given channel
    pub fn new_3d(x: PixelAddress, y: PixelAddress, c: u8) -> Self {
        Self { x, y, c: Some(c) }
    }

    /// Returns the x coordinate as a [`PixelAddress`]
    pub fn x(&self) -> PixelAddress {
        self.x
    }

    /// Returns the y coordinate as a [`PixelAddress`]
    pub fn y(&self) -> PixelAddress {
        self.y
    }

    /// Returns the channel as an `Option<u8>`
    pub fn c(&self) -> Option<u8> {
        self.c
    }

    /// Returns the x coordinate as a `usize`
    pub fn x_usize(&self) -> usize {
        self.x as usize
    }

    /// Returns the y coordinate as a `usize`
    pub fn y_usize(&self) -> usize {
        self.y as usize
    }

    /// Returns the channel as a usize, or 0 if the coordinate is 2D
    pub fn c_usize(&self) -> usize {
        self.c.unwrap_or(0) as usize
    }

    /// Returns true if the coordinate is 2D
    pub fn is_2d(&self) -> bool {
        self.c.is_none()
    }

    /// Returns true if the coordinate is 3D
    pub fn is_3d(&self) -> bool {
        self.c.is_some()
    }

    /// Returns true if the coordinate is valid
    pub fn is_valid(&self) -> bool {
        self.x != EOF_PX_ADDRESS && self.y != EOF_PX_ADDRESS
    }

    /// Returns true if the coordinate is the EOF coordinate
    pub fn is_eof(&self) -> bool {
        self.x == EOF_PX_ADDRESS && self.y == EOF_PX_ADDRESS
    }
}

/// A 2D coordinate representation
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CoordSingle {
    pub x: PixelAddress,
    pub y: PixelAddress,
}

/// An ADΔER event representation
#[allow(missing_docs)]
#[repr(packed)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct Event {
    pub coord: Coord,
    pub d: D,
    pub delta_t: DeltaT,
}

/// An ADΔER event representation, without the channel component
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventSingle {
    pub coord: CoordSingle,
    pub d: D,
    pub delta_t: DeltaT,
}

impl From<&Event> for EventSingle {
    fn from(event: &Event) -> Self {
        EventSingle {
            coord: CoordSingle {
                x: event.coord.x,
                y: event.coord.y,
            },
            d: event.d,
            delta_t: event.delta_t,
        }
    }
}

impl From<EventSingle> for Event {
    fn from(event: EventSingle) -> Self {
        Event {
            coord: Coord {
                x: event.coord.x,
                y: event.coord.y,
                c: None,
            },
            d: event.d,
            delta_t: event.delta_t,
        }
    }
}

/// The type of data source representation
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SourceType {
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

const EOF_EVENT: Event = Event {
    coord: Coord {
        x: EOF_PX_ADDRESS,
        y: EOF_PX_ADDRESS,
        c: Some(0),
    },
    d: 0,
    delta_t: 0,
};

/// Helper function for opening a file as a raw or compressed input ADΔER stream
pub fn open_file_decoder(
    file_path: &str,
) -> Result<
    (
        Decoder<BufReader<File>>,
        BitReader<BufReader<File>, BigEndian>,
    ),
    CodecError,
> {
    let mut bufreader = BufReader::new(File::open(file_path)?);
    let compression = RawInput::new();
    let mut bitreader = BitReader::endian(bufreader, BigEndian);

    // First try opening the file as a raw file, then try as a compressed file
    let stream = match Decoder::new_raw(compression, &mut bitreader) {
        Ok(reader) => reader,
        Err(CodecError::WrongMagic) => {
            bufreader = BufReader::new(File::open(file_path)?);
            let compression = CompressedInput::new();
            bitreader = BitReader::endian(bufreader, BigEndian);
            Decoder::new_compressed(compression, &mut bitreader)?
        }
        Err(e) => {
            return Err(e);
        }
    };
    Ok((stream, bitreader))
}

/// An ADΔER event representation
#[allow(missing_docs)]
#[derive(Debug, Copy, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct EventCoordless {
    pub d: D,

    pub delta_t: DeltaT,
}

impl EventCoordless {
    #[inline(always)]
    pub fn t(&self) -> AbsoluteT {
        self.delta_t as AbsoluteT
    }
}

impl From<Event> for EventCoordless {
    fn from(event: Event) -> Self {
        Self {
            d: event.d,
            delta_t: event.delta_t,
        }
    }
}

impl Add<EventCoordless> for EventCoordless {
    type Output = EventCoordless;

    fn add(self, _rhs: EventCoordless) -> EventCoordless {
        todo!()
    }
}

impl num_traits::Zero for EventCoordless {
    fn zero() -> Self {
        EventCoordless { d: 0, delta_t: 0 }
    }

    fn is_zero(&self) -> bool {
        self.d.is_zero() && self.delta_t.is_zero()
    }
}
