#![allow(dead_code, unused_imports)]

use std::{default, fmt::Display, marker};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    NoStartOfImage,
    InvalidMarker,
    UnknownMarker(u8),
    MultipleSOI,
    InvalidAPP0Marker,
    NoData,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JPEG Error: {}",
            match self {
                Self::NoStartOfImage => "JPEG has no Start of Image marker".to_string(),
                Self::NoData => "No Data after Start of Image marker".to_string(),
                Self::InvalidMarker => "A 0xFF was found with no code after it".to_string(),
                Self::InvalidAPP0Marker => "The APP0 marker has invalid data".to_string(),
                Self::UnknownMarker(marker) =>
                    format!("An unknown marker 0x{:04X} was encountered", marker),
                Self::MultipleSOI => "Encountered multiple Start of Image markers".to_string(),
            }
        )
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Marker {
    SOI,
    EOI,
    //Padding,
    APP0,
    DQT,
}

impl Eq for Marker {}

impl Marker {
    const HEX_SOI: u8 = 0xD8;
    const HEX_EOI: u8 = 0xD9;
    const HEX_PADDING: u8 = 0x00;

    fn hex(&self) -> u8 {
        match self {
            Self::SOI => 0xD8,
            Self::EOI => 0xD9,
            //Self::Padding => 0x00,
            Self::APP0 => 0xE0,
            Self::DQT => 0xDB,
        }
    }

    fn marker(byte: u8) -> Option<Self> {
        match byte {
            0xD8 => Some(Self::SOI),
            0xD9 => Some(Self::EOI),
            //0x00 => Some(Self::Padding),
            0xE0 => Some(Self::APP0),
            0xDB => Some(Self::DQT),
            _ => None,
        }
    }

    fn process(&self, stream: &mut impl Iterator<Item = u8>, jpeg: &mut JPEG) -> Result<(), Error> {
        match self {
            //Self::Padding => Ok(()),
            Self::SOI => Ok(()),
            Self::EOI => Ok(()),
            Self::DQT => Ok(()),
            Self::APP0 => {
                let error = Error::InvalidAPP0Marker;

                let l1 = stream.next().ok_or(error)?;
                let l2 = stream.next().ok_or(error)?;

                let length = ((l1 as i16) << 8) | (l2 as i16);
                let mut length = length - 2;

                // Skip till 4th byte of identifier
                for _ in 0..3 {
                    stream.next();
                }

                let is_extension = stream.next().ok_or(error)? == 0x58;
                stream.next();
                length -= 5; // Reduce by length of identifier

                if !is_extension {
                    if jpeg.jfif.is_some() {
                        dbg!("Multiple non-extension JFIF segment markers encountered!");
                        return Ok(());
                    }
                    let major_version = stream.next().ok_or(error)?;
                    let minor_version = stream.next().ok_or(error)?;

                    let units = stream.next().ok_or(error)?;

                    let units = match units {
                        0x00 => JfifUnit::NoUnit,
                        0x01 => JfifUnit::PerInch,
                        0x02 => JfifUnit::PerCenti,
                        _ => return Err(error),
                    };

                    let x_density = {
                        let f = stream.next().ok_or(error)?;
                        let s = stream.next().ok_or(error)?;

                        ((f as u16) << 8) | (s as u16)
                    };

                    let y_density = {
                        let f = stream.next().ok_or(error)?;
                        let s = stream.next().ok_or(error)?;

                        ((f as u16) << 8) | (s as u16)
                    };

                    let x_thumbnail = stream.next().ok_or(error)?;
                    let y_thumbnail = stream.next().ok_or(error)?;

                    let mut thumbnail_data = Vec::with_capacity(length as usize);

                    length -= 9;

                    for _ in 0..length {
                        let byte = stream.next().ok_or(error)?;
                        thumbnail_data.push(byte);
                    }

                    let ap = APP0 {
                        major_version,
                        minor_version,
                        units,
                        x_density,
                        y_density,
                        x_thumbnail,
                        y_thumbnail,
                        thumbnail_data,
                    };

                    jpeg.jfif = Some(ap);
                } else {
                    for _ in 0..length {
                        stream.next();
                    }
                }

                Ok(())
            }
        }
    }

    fn read(stream: &mut impl Iterator<Item = u8>, jpeg: &mut JPEG) -> Result<(), Error> {
        // Guaranteed by check in JPEG::new
        let marker = stream.next().unwrap();

        println!("Reading {:04X} marker", marker);

        match Self::marker(marker) {
            Some(marker) => {
                if marker == Self::SOI {
                    return Err(Error::MultipleSOI);
                }
                marker.process(stream, jpeg)
            }
            None => Err(Error::UnknownMarker(marker)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum JfifUnit {
    #[default]
    NoUnit,
    PerInch,
    PerCenti,
}

#[derive(Clone, Debug, PartialEq, Default)]
struct APP0 {
    major_version: u8,
    minor_version: u8,
    units: JfifUnit,
    x_density: u16,
    y_density: u16,
    x_thumbnail: u8,
    y_thumbnail: u8,
    thumbnail_data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct JPEG {
    jfif: Option<APP0>,
}

impl JPEG {
    pub fn new(stream: Vec<u8>) -> Result<JPEG, Error> {
        let mut stream = stream.into_iter();

        let mut has_soi = false;

        // Advance until SOI
        while let Some(byte) = stream.next() {
            if byte == 0xFF && Some(Marker::HEX_SOI) == stream.next() {
                has_soi = true;
                break;
            }
        }

        if !has_soi {
            return Err(Error::NoStartOfImage);
        }

        let mut stream = stream.peekable();

        if stream.peek().is_none() {
            return Err(Error::NoData);
        }

        let mut jpeg = JPEG { jfif: None };

        // Advance until next marker
        while let Some(byte) = stream.next() {
            if byte == 0xFF {
                if stream.peek().is_some() {
                    let _res = Marker::read(&mut stream, &mut jpeg)?;
                } else {
                    return Err(Error::InvalidMarker);
                }
            }
        }

        Ok(jpeg)
    }
}
