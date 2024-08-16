#![allow(dead_code, unused_imports)]

use std::{default, fmt::Display, marker, usize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SOF0MarkerError {
    MissingNextByte,
    InvalidComponentNumber,
    InvalidComponentID,
    ComponentAlreadySet,
    UnsupportedComponentQTable,
    InvalidMarkerLength,
    InvalidPrecision,
}

impl Display for SOF0MarkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Start of Frame Error: {}",
            match self {
                Self::InvalidComponentID => "Component has invalid or unsupported id",
                Self::ComponentAlreadySet => "Tried to overwrite set component",
                Self::InvalidMarkerLength =>
                    "Stated Marker Length does not match actual component length",
                Self::UnsupportedComponentQTable => "Component uses unsupported QTable",
                Self::InvalidPrecision => "Marker has invalid precision",
                Self::MissingNextByte => "Missing next byte in marker",
                Self::InvalidComponentNumber => "Number of components is invalid or unsupported",
            }
        )
    }
}

impl std::error::Error for SOF0MarkerError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DQTError {
    MissingNextByte,
    InvalidTableDestination,
    NoTableSet,
}

impl Display for DQTError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Define Quantization Table Error: {}",
            match self {
                Self::MissingNextByte => "Missing next byte in marker",
                Self::InvalidTableDestination => "QTable Destination is greater than 0x03",
                Self::NoTableSet => "Marker did not set any QTable",
            }
        )
    }
}

impl std::error::Error for DQTError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    StartOfImageNotFound,
    StartOfFrameNotFound,
    QTableNotFound,
    InvalidMarker,
    UnknownMarker(u8),
    MultipleSOI,
    MultipleSOF,
    InvalidAPP0Marker,
    InvalidDQTMarker(DQTError),
    InvalidSOF0Marker(SOF0MarkerError),
    NoData,
    DataAfterEOI,
    InvalidRestartIntervalMarker,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JPEG Error: {}",
            match self {
                Self::StartOfImageNotFound => "JPEG has no Start of Image marker".to_string(),
                Self::StartOfFrameNotFound => "JPEG has no Start of Frame marker".to_string(),
                Self::QTableNotFound => "JPEG has no DQT marker".to_string(),
                Self::NoData => "No Data after Start of Image marker".to_string(),
                Self::DataAfterEOI => "Data found after End of Image marker".to_string(),
                Self::InvalidMarker => "A 0xFF was found with no code after it".to_string(),
                Self::InvalidAPP0Marker => "The APP0 marker has invalid data".to_string(),
                Self::InvalidRestartIntervalMarker => "The DRI marker has invalid data".to_string(),
                Self::InvalidDQTMarker(source) =>
                    format!("The DQT marker has invalid data. {}", source),
                Self::InvalidSOF0Marker(source) =>
                    format!("The baseline SOF marker has invalid data. {}", source),
                Self::UnknownMarker(marker) =>
                    format!("An unknown marker 0x{:04X} was encountered", marker),
                Self::MultipleSOI => "Encountered multiple Start of Image markers".to_string(),
                Self::MultipleSOF => "Encountered multiple Start of Frame markers".to_string(),
            }
        )
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidSOF0Marker(source) => Some(source),
            Self::InvalidDQTMarker(source) => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Marker {
    SOI,
    EOI,
    //Padding,
    APP0,
    DQT,
    SOF0,
    DRI,
    APPN,
}

impl Eq for Marker {}

impl Marker {
    const HEX_SOI: u8 = 0xD8;
    const HEX_EOI: u8 = 0xD9;
    const HEX_PADDING: u8 = 0x00;

    fn marker(byte: u8) -> Option<Self> {
        match byte {
            0xD8 => Some(Self::SOI),
            0xD9 => Some(Self::EOI),
            //0x00 => Some(Self::Padding),
            0xE0 => Some(Self::APP0),
            0xDB => Some(Self::DQT),
            0xC0 => Some(Self::SOF0),
            0xDD => Some(Self::DRI),
            0xEE..=0xEF => Some(Self::APPN),
            _ => None,
        }
    }

    fn process(
        &self,
        stream: &mut impl Iterator<Item = u8>,
        jpeg: &mut JPEG,
    ) -> Result<Outcome, Error> {
        match self {
            //Self::Padding => Ok(()),
            Self::SOI => Ok(Outcome::None),
            Self::EOI => Ok(Outcome::EndOfImage),
            Self::APPN => {
                let error = Error::InvalidMarker;
                let length = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    let len = ((x as i16) << 8) | (y as i16);

                    len - 2
                };

                for _ in 0..length {
                    stream.next();
                }

                Ok(Outcome::None)
            }
            Self::DRI => {
                let error = Error::InvalidRestartIntervalMarker;
                let length = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as u16) << 8) | (y as u16)
                };

                if length != 0x04 {
                    return Err(Error::InvalidRestartIntervalMarker);
                }

                let rsi = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as u16) << 8) | (y as u16)
                };

                jpeg.restart_interval = rsi;

                Ok(Outcome::None)
            }
            Self::SOF0 => {
                if jpeg.base_line_sof.is_set {
                    return Err(Error::MultipleSOF);
                }

                fn throw(error: SOF0MarkerError) -> Result<Outcome, Error> {
                    return Err(Error::InvalidSOF0Marker(error));
                }

                let error = Error::InvalidSOF0Marker(SOF0MarkerError::MissingNextByte);

                let length = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as i16) << 8) | (y as i16)
                };

                let precision = stream.next().ok_or(error)?; // Base line SOF0 always has 8 precision
                if precision != 0x08 {
                    return throw(SOF0MarkerError::InvalidPrecision);
                }

                let height = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as u16) << 8) | (y as u16)
                };

                let width = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as u16) << 8) | (y as u16)
                };

                let component_number = stream.next().ok_or(error)?;

                if component_number == 0x00 || component_number == 0x02 {
                    return throw(SOF0MarkerError::InvalidComponentNumber);
                }

                let component_number = component_number.clamp(1, 4);

                let mut baseline = BaseLineSOF::default();
                baseline.width = width;
                baseline.height = height;

                for _ in 0..component_number {
                    let id = stream.next().ok_or(error)? as u8;

                    if id == 0x00 {
                        return throw(SOF0MarkerError::InvalidComponentID);
                    }

                    if id > 0x04 {
                        // larger ids are not supported
                        return throw(SOF0MarkerError::InvalidComponentID);
                    }

                    let idx = (id - 1) as usize;

                    let component = baseline.components.get_mut(idx).unwrap();

                    if component.is_set {
                        return throw(SOF0MarkerError::ComponentAlreadySet);
                    }

                    let (hfactor, vfactor) = {
                        let factor = stream.next().ok_or(error)?;
                        (factor >> 4, factor & 0x0F)
                    };

                    let qtable = stream.next().ok_or(error)?;

                    if qtable > 0x03 {
                        return throw(SOF0MarkerError::UnsupportedComponentQTable);
                    }

                    component.id = id;
                    component.hfactor = hfactor;
                    component.vfactor = vfactor;
                    component.qtable = qtable;
                    component.is_set = true;
                }

                baseline.is_set = true;

                jpeg.base_line_sof = baseline;

                if length - 8 - (3 * (component_number as i16)) != 0 {
                    return throw(SOF0MarkerError::InvalidMarkerLength);
                }

                //Make sure at least 1 component is set
                Ok(Outcome::StartOfFrame)
            }
            Self::DQT => {
                let error = Error::InvalidDQTMarker(DQTError::MissingNextByte);
                let mut length = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    let length = ((x as i16) << 8) | (y as i16);

                    length - 2
                };

                // Accumulate tables
                while length > 0 {
                    let id = stream.next().ok_or(error)?;
                    length -= 1;

                    let (is_extended, kind) = { (id >> 4 == 1, id & 0x0F) };

                    let qtable_type = match kind {
                        0x00 => QTableType::Luminance,
                        0x01 => QTableType::Chrominance,
                        0x02 | 3 => QTableType::Other,
                        _ => {
                            return Err(Error::InvalidDQTMarker(DQTError::InvalidTableDestination))
                        }
                    };

                    let mut data = [0; 64];

                    if is_extended {
                        for i in 0..64 {
                            let x = stream.next().ok_or(error)?;
                            let y = stream.next().ok_or(error)?;

                            data[QTable::ZIGZAG[i] as usize] = ((x as u16) << 8) | (y as u16);
                        }

                        length -= 128;
                    } else {
                        for i in 0..64 {
                            let byte = stream.next().ok_or(error)?;
                            data[QTable::ZIGZAG[i] as usize] = byte as u16;
                        }

                        length -= 64;
                    }

                    let qtable = QTable {
                        is_set: true,
                        is_extended_mode: is_extended,
                        kind: qtable_type,
                        table: data,
                    };

                    // Kind being out of range should be caught by qtable_type
                    jpeg.qtables[kind as usize] = qtable;
                }

                // At least one QTable Must be set
                if jpeg.qtables.iter().find(|table| table.is_set).is_none() {
                    return Err(Error::InvalidDQTMarker(DQTError::NoTableSet));
                }

                Ok(Outcome::QTableSet)
            }
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
                        return Ok(Outcome::None);
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

                Ok(Outcome::None)
            }
        }
    }

    fn read(stream: &mut impl Iterator<Item = u8>, jpeg: &mut JPEG) -> Result<Outcome, Error> {
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum QTableType {
    Luminance,
    Chrominance,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct QTable {
    is_set: bool,
    is_extended_mode: bool,
    kind: QTableType,
    table: [u16; 64],
}

impl QTable {
    const ZIGZAG: [u16; 64] = [
        0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
        20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
        58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
    ];
}

impl Default for QTable {
    fn default() -> Self {
        Self {
            is_set: false,
            is_extended_mode: false,
            kind: QTableType::Other,
            table: [0; 64],
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct SOFComponent {
    id: u8,
    hfactor: u8,
    vfactor: u8,
    qtable: u8,
    is_set: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct BaseLineSOF {
    height: u16,
    width: u16,
    components: [SOFComponent; 4],
    is_set: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    None,
    EndOfImage,
    QTableSet,
    StartOfFrame,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct JPEG {
    jfif: Option<APP0>,
    qtables: [QTable; 4],
    base_line_sof: BaseLineSOF,
    restart_interval: u16,
}

impl JPEG {
    pub fn new(stream: Vec<u8>) -> Result<JPEG, Error> {
        let mut stream = stream.into_iter();

        let mut has_soi = false;
        let mut has_sof = false;
        let mut has_qtable = false;

        // Advance until SOI
        while let Some(byte) = stream.next() {
            if byte == 0xFF && Some(Marker::HEX_SOI) == stream.next() {
                has_soi = true;
                break;
            }
        }

        if !has_soi {
            return Err(Error::StartOfImageNotFound);
        }

        let mut stream = stream.peekable();

        if stream.peek().is_none() {
            return Err(Error::NoData);
        }

        // TODO: Might want to implement default for JPEG later
        let mut jpeg = JPEG {
            jfif: None,
            qtables: [QTable::default(); 4],
            base_line_sof: BaseLineSOF::default(),
            restart_interval: 0,
        };

        // Advance until next marker
        while let Some(byte) = stream.next() {
            if byte == 0xFF {
                if stream.peek().is_some() {
                    match Marker::read(&mut stream, &mut jpeg)? {
                        Outcome::EndOfImage => {
                            if stream.peek().is_some() {
                                return Err(Error::DataAfterEOI);
                            } else {
                                break;
                            }
                        }
                        Outcome::StartOfFrame => {
                            has_sof = true;
                        }
                        Outcome::QTableSet => {
                            has_qtable = true;
                        }
                        Outcome::None => {}
                    };
                } else {
                    return Err(Error::InvalidMarker);
                }
            }
        }

        if !has_sof {
            return Err(Error::StartOfFrameNotFound);
        }
        if !has_qtable {
            return Err(Error::QTableNotFound);
        }

        // need to check if at least 1 QTable is set

        Ok(jpeg)
    }
}
