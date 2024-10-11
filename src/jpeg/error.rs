use std::{error, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SOF0MarkerError {
    MissingNextByte,
    InvalidComponentNumber,
    ZeroDimensions,
    InvalidComponentID,
    ComponentAlreadySet,
    UnsupportedComponentQTable,
    InvalidMarkerLength,
    InvalidPrecision,
    NoComponentSet,
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
                Self::ZeroDimensions => "Marker has width or height set to zero",
                Self::MissingNextByte => "Missing next byte in marker",
                Self::InvalidComponentNumber => "Number of components is invalid or unsupported",
                Self::NoComponentSet => "No component was set by marker",
            }
        )
    }
}

impl error::Error for SOF0MarkerError {}

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

impl error::Error for DQTError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DHTError {
    MissingNextByte,
    InvalidMarkerLength,
    InvalidTableId,
    InvalidSymbolsLength,
    NoTableSet,
}

impl Display for DHTError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Define Huffman Table Error: {}.",
            match self {
                Self::MissingNextByte => "Missing next byte in marker",
                Self::InvalidMarkerLength => "Stated marker length does not match actual length",
                Self::InvalidTableId => "A table has an invalid table ID",
                Self::InvalidSymbolsLength => "A table has more symbols than allowed",
                Self::NoTableSet => "No Huffman table was set by marker",
            }
        )
    }
}

impl error::Error for DHTError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SOSError {
    MissingNextByte,
    InvalidMarkerLength,
    InvalidOrder,
    InvalidComponentNumber,
    InvalidComponentID,
    DuplicateComponentID,
    InvalidHuffmanTableID,
    InvalidSpectralSelection,
    InvalidSuccesiveApproximation,
}

impl Display for SOSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Start of Scan Error: {}.",
            match self {
                Self::MissingNextByte => "Missing next byte in marker",
                Self::InvalidMarkerLength => "Stated marker length does not match actual length",
                Self::InvalidOrder => "Start of Scan reached before Start of Frame",
                Self::InvalidComponentNumber => "Invalid number of components",
                Self::InvalidComponentID => "Invalid component ID",
                Self::DuplicateComponentID => "Multiple components have the same id",
                Self::InvalidHuffmanTableID => "A Huffman table id greater than 3 was reached",
                Self::InvalidSpectralSelection =>
                    "Either the starting or ending spectral selection is out of bounds",
                Self::InvalidSuccesiveApproximation =>
                    "The successive approximation is out of bounds",
            }
        )
    }
}

impl error::Error for SOSError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HuffmanDecodingError {
    ReadPastLength,
    SymbolNotFound,
    InvalidDCCoefficientLength,
    ZerosExceedMCULength,
    InvalidACCoefficientLength,
}

impl Display for HuffmanDecodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::ReadPastLength => "Entire Huffman bit stream read",
                Self::SymbolNotFound => "Symbol not found after reading past 16 bits",
                Self::InvalidDCCoefficientLength => "DC coefficient had length greater than 11",
                Self::InvalidACCoefficientLength => "AC coefficient had length greater than 10",
                Self::ZerosExceedMCULength => "AC Table Zeroes exceeded run length of MCU",
            }
        )
    }
}

impl error::Error for HuffmanDecodingError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    StartOfImageNotFound,
    StartOfFrameNotFound,
    QTableNotFound,
    HTableNotFound,
    SOSNotFound,
    InvalidMarker,
    UnknownMarker(u8),
    MultipleSOI,
    MultipleSOF,
    InvalidAPP0Marker,
    InvalidDQTMarker(DQTError),
    InvalidSOF0Marker(SOF0MarkerError),
    InvalidDHTMarker(DHTError),
    InvalidSOSMarker(SOSError),
    NoData,
    InvalidRestartIntervalMarker,
    RestartMarkerBeforeSOS,
    EndOfImageBeforeSOS,
    PrematureEnd,
    InvalidColorComponent,
    HuffmanDecode(HuffmanDecodingError),
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
                Self::HTableNotFound => "JPEG has no DHT marker".to_string(),
                Self::InvalidColorComponent =>
                    "A color component was not correctly set".to_string(),
                Self::SOSNotFound => "JPEG has no SOS marker".to_string(),
                Self::NoData => "No Data after Start of Image marker".to_string(),
                Self::PrematureEnd => "The file ended prematurely".to_string(),
                Self::InvalidMarker => "A 0xFF was found with no code after it".to_string(),
                Self::InvalidAPP0Marker => "The APP0 marker has invalid data".to_string(),
                Self::InvalidRestartIntervalMarker => "The DRI marker has invalid data".to_string(),
                Self::InvalidDQTMarker(source) =>
                    format!("The DQT marker has invalid data. {}", source),
                Self::InvalidSOF0Marker(source) =>
                    format!("The baseline SOF marker has invalid data. {}", source),
                Self::InvalidDHTMarker(source) =>
                    format!("The DHT marker has invalid data. {}", source),
                Self::InvalidSOSMarker(source) =>
                    format!("The SOS marker has invalid data. {}", source),
                Self::UnknownMarker(marker) =>
                    format!("An unknown marker 0x{:04X} was encountered", marker),
                Self::MultipleSOI => "Encountered multiple Start of Image markers".to_string(),
                Self::MultipleSOF => "Encountered multiple Start of Frame markers".to_string(),
                Self::RestartMarkerBeforeSOS =>
                    "Encountered a Restart Marker before a Start of Scan marker".to_string(),
                Self::EndOfImageBeforeSOS =>
                    "Encountered an End of Image marker before a Start of Scan marker".to_string(),
                Self::HuffmanDecode(source) => source.to_string(),
            }
        )
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::InvalidSOF0Marker(source) => Some(source),
            Self::InvalidDQTMarker(source) => Some(source),
            Self::InvalidDHTMarker(source) => Some(source),
            Self::InvalidSOSMarker(source) => Some(source),
            _ => None,
        }
    }
}

impl From<HuffmanDecodingError> for Error {
    fn from(value: HuffmanDecodingError) -> Self {
        Error::HuffmanDecode(value)
    }
}

pub type Result<T> = core::result::Result<T, Error>;
