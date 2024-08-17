use super::error::*;
use std::{iter::Peekable, usize};

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
    SOFN,
    DHT,
    SOS,
    JPGEXT,
    DAC,
    RSTN,
    DNL,
    DHP,
    EXP,
    APP1,
    JPG,
    COM,
    TEM,
}

impl Eq for Marker {}

impl Marker {
    const HEX_SOI: u8 = 0xD8;
    const HEX_EOI: u8 = 0xD9;

    /// Length without the subtraction
    fn marker_length(stream: &mut impl Iterator<Item = u8>, error: Error) -> Result<u16> {
        let x = stream.next().ok_or(error)?;
        let y = stream.next().ok_or(error)?;

        Ok(((x as u16) << 8) | (y as u16))
    }

    fn marker(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(Self::TEM),
            0xD8 => Some(Self::SOI),
            0xD9 => Some(Self::EOI),
            0xE0 => Some(Self::APP0),
            0xDB => Some(Self::DQT),
            0xC0 => Some(Self::SOF0),
            0xC4 => Some(Self::DHT),
            0xDD => Some(Self::DRI),
            0xDA => Some(Self::SOS),
            0xC8 => Some(Self::JPGEXT),
            0xCC => Some(Self::DAC),
            0xC1..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCE..=0xCF => Some(Self::SOFN),
            0xD0..=0xD7 => Some(Self::RSTN),
            0xDC => Some(Self::DNL),
            0xDE => Some(Self::DHP),
            0xDF => Some(Self::EXP),
            0xE1 => Some(Self::APP1),
            0xE2..=0xEF => Some(Self::APPN),
            0xF0..=0xFD => Some(Self::JPG),
            0xFE => Some(Self::COM),
            _ => None,
        }
    }

    fn skip_sized_marker(stream: &mut impl Iterator<Item = u8>) -> Result<DecodingOutcome> {
        let error = Error::InvalidMarker;
        let length = Self::marker_length(stream, error)? - 2;

        for _ in 0..length {
            stream.next();
        }

        Ok(DecodingOutcome::None)
    }

    fn process(
        &self,
        stream: &mut impl Iterator<Item = u8>,
        jpeg: &mut JPEGHeader,
    ) -> Result<DecodingOutcome> {
        match self {
            //Self::Padding => Ok(()),
            Self::TEM => Ok(DecodingOutcome::None),
            Self::SOI => Ok(DecodingOutcome::None),
            Self::EOI => Err(Error::EndOfImageBeforeSOS),
            Self::RSTN => Err(Error::RestartMarkerBeforeSOS),
            Self::APPN => Self::skip_sized_marker(stream),
            Self::SOFN => Self::skip_sized_marker(stream),
            Self::JPGEXT => Self::skip_sized_marker(stream),
            Self::DAC => Self::skip_sized_marker(stream),
            Self::DNL => Self::skip_sized_marker(stream),
            Self::DHP => Self::skip_sized_marker(stream),
            Self::EXP => Self::skip_sized_marker(stream),
            Self::JPG => Self::skip_sized_marker(stream),
            Self::COM => Self::skip_sized_marker(stream),
            Self::APP1 => todo!("EXIF needs implementing"),
            Self::SOS => {
                let error = Error::InvalidSOSMarker(SOSError::MissingNextByte);

                fn throw(error: SOSError) -> Result<DecodingOutcome> {
                    return Err(Error::InvalidSOSMarker(error));
                }

                if jpeg
                    .components
                    .iter()
                    .find(|component| component.is_used_sof)
                    .is_none()
                {
                    return throw(SOSError::InvalidOrder);
                }

                let length = Self::marker_length(stream, error)? as i16;

                let component_number = stream.next().ok_or(error)?;

                if component_number == 0x00 || component_number > 0x04 {
                    return throw(SOSError::InvalidComponentNumber);
                }

                for _ in 0..component_number {
                    let mut component_id = stream.next().ok_or(error)?;

                    if jpeg.zero_based_component_id {
                        component_id += 1;
                    }

                    if component_id > 0x04 {
                        return throw(SOSError::InvalidComponentID);
                    }

                    let component = &mut jpeg.components[(component_id as usize) - 1];

                    if component.is_used_sos {
                        return throw(SOSError::DuplicateComponentID);
                    }

                    component.is_used_sos = true;

                    let htable_ids = stream.next().ok_or(error)?;
                    let dc_id = htable_ids >> 4;
                    let ac_id = htable_ids & 0x0F;

                    if dc_id > 0x03 || ac_id > 0x03 {
                        return throw(SOSError::InvalidHuffmanTableID);
                    }
                }

                let selection_start = stream.next().ok_or(error)?;
                let selection_end = stream.next().ok_or(error)?;

                if selection_start > 0x3F || selection_end > 0x3F {
                    return throw(SOSError::InvalidSpectralSelection);
                }

                jpeg.start_of_selection = selection_start;
                jpeg.end_of_selection = selection_end;

                let approximation = stream.next().ok_or(error)?;
                let high = approximation >> 4;
                let low = approximation & 0x0F;

                if high > 13 {
                    return throw(SOSError::InvalidSuccesiveApproximation);
                }

                jpeg.successive_approximation_high = high;
                jpeg.successive_approximation_low = low;

                if length - 6 - 2 * (component_number as i16) != 0 {
                    return throw(SOSError::InvalidMarkerLength);
                }

                Ok(DecodingOutcome::StartOfScan)
            }
            Self::DHT => {
                let error = Error::InvalidDHTMarker(DHTError::MissingNextByte);
                fn throw(error: DHTError) -> Result<DecodingOutcome> {
                    return Err(Error::InvalidDHTMarker(error));
                }

                let mut length = (Self::marker_length(stream, error)? as i16) - 2;

                while length > 0 {
                    let table_info = stream.next().ok_or(error)?;
                    let table_id = table_info & 0x0F;
                    let is_ac = table_info >> 4 == 0x01;

                    if table_id > 0x03 {
                        return throw(DHTError::InvalidTableId);
                    }

                    let htable = if is_ac {
                        &mut jpeg.huffman_tables_ac[table_id as usize]
                    } else {
                        &mut jpeg.huffman_tables_dc[table_id as usize]
                    };

                    let mut total_symbols = 0;

                    for i in 1..17 {
                        total_symbols += stream.next().ok_or(error)?;
                        htable.offsets[i] = total_symbols;
                    }

                    if total_symbols > 0xA2 {
                        return throw(DHTError::InvalidSymbolsLength);
                    }

                    for i in 0..total_symbols {
                        htable.symbols[i as usize] = stream.next().ok_or(error)?;
                    }

                    htable.is_set = true;
                    length -= 17 + (total_symbols as i16);
                }

                if jpeg
                    .huffman_tables_ac
                    .iter()
                    .chain(jpeg.huffman_tables_dc.iter())
                    .find(|htable| htable.is_set)
                    .is_none()
                {
                    return throw(DHTError::NoTableSet);
                }

                if length != 0 {
                    return throw(DHTError::InvalidMarkerLength);
                }

                Ok(DecodingOutcome::HuffmanTable)
            }
            Self::DRI => {
                let error = Error::InvalidRestartIntervalMarker;
                let length = Self::marker_length(stream, error)?;

                if length != 0x04 {
                    return Err(Error::InvalidRestartIntervalMarker);
                }

                let rsi = {
                    let x = stream.next().ok_or(error)?;
                    let y = stream.next().ok_or(error)?;

                    ((x as u16) << 8) | (y as u16)
                };

                jpeg.restart_interval = rsi;

                Ok(DecodingOutcome::None)
            }
            Self::SOF0 => {
                if jpeg.is_sof_set {
                    return Err(Error::MultipleSOF);
                }

                fn throw(error: SOF0MarkerError) -> Result<DecodingOutcome> {
                    return Err(Error::InvalidSOF0Marker(error));
                }

                let error = Error::InvalidSOF0Marker(SOF0MarkerError::MissingNextByte);

                let length = Self::marker_length(stream, error)? as i16;

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

                if width == 0 || height == 0 {
                    return throw(SOF0MarkerError::ZeroDimensions);
                }

                let component_number = stream.next().ok_or(error)?;

                if component_number == 0x00 || component_number == 0x02 {
                    return throw(SOF0MarkerError::InvalidComponentNumber);
                }

                let component_number = component_number.clamp(1, 4);

                jpeg.width = width;
                jpeg.height = height;

                for _ in 0..component_number {
                    let mut id = stream.next().ok_or(error)? as u8;

                    if id == 0x00 {
                        jpeg.zero_based_component_id = true;
                    }

                    if jpeg.zero_based_component_id {
                        id += 1;
                    }

                    if id == 0x00 {
                        return throw(SOF0MarkerError::InvalidComponentID);
                    }

                    if id > 0x04 {
                        // larger ids are not supported
                        return throw(SOF0MarkerError::InvalidComponentID);
                    }

                    let idx = (id - 1) as usize;

                    let component = jpeg.components.get_mut(idx).unwrap();

                    if component.is_used_sof {
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
                    component.is_used_sof = true;
                }

                jpeg.is_sof_set = true;

                if length - 8 - (3 * (component_number as i16)) != 0 {
                    return throw(SOF0MarkerError::InvalidMarkerLength);
                }

                //Make sure at least 1 component is set
                if jpeg
                    .components
                    .iter()
                    .find(|component| component.is_used_sof)
                    .is_none()
                {
                    return throw(SOF0MarkerError::NoComponentSet);
                }

                Ok(DecodingOutcome::StartOfFrame)
            }
            Self::DQT => {
                let error = Error::InvalidDQTMarker(DQTError::MissingNextByte);
                let mut length = (Self::marker_length(stream, error)? as i16) - 2;

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

                Ok(DecodingOutcome::QTableSet)
            }
            Self::APP0 => {
                let error = Error::InvalidAPP0Marker;

                let mut length = (Self::marker_length(stream, error)? as i16) - 2;

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
                        return Ok(DecodingOutcome::None);
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

                Ok(DecodingOutcome::None)
            }
        }
    }

    fn scan<I>(stream: &mut Peekable<I>, jpeg: &mut JPEGHeader) -> Result<()>
    where
        I: Iterator<Item = u8>,
    {
        loop {
            match stream.next() {
                None => return Err(Error::PrematureEnd),
                Some(current) => {
                    if current == 0xFF {
                        let next = stream.peek();

                        if next == Some(&Marker::HEX_EOI) {
                            break;
                        } else if next == Some(&0x00) {
                            jpeg.huffman_data.push(current);
                            stream.next();
                        } else if &0xD0 <= next.ok_or(Error::PrematureEnd)?
                            || next.ok_or(Error::PrematureEnd)? <= &0xD7
                        {
                            stream.next();
                        }
                    } else {
                        jpeg.huffman_data.push(current);
                    }
                }
            }
        }

        Ok(())
    }

    fn read<I>(stream: &mut Peekable<I>, jpeg: &mut JPEGHeader) -> Result<DecodingOutcome>
    where
        I: Iterator<Item = u8>,
    {
        // Skip repetitions of 0xFF
        while let Some(marker) = stream.peek() {
            if *marker != 0xFF {
                break;
            } else {
                stream.next();
            }
        }

        let marker = stream.next().ok_or(Error::InvalidMarker)?;

        println!("Reading 0x{:02X} marker", marker);

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
struct ColorComponent {
    id: u8,
    hfactor: u8,
    vfactor: u8,
    qtable: u8,
    huffman_table_ac_id: u8,
    huffman_table_dc_id: u8,
    is_used_sof: bool,
    is_used_sos: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HuffmanTable {
    offsets: [u8; 17],
    symbols: [u8; 162],
    is_set: bool,
}

impl Default for HuffmanTable {
    fn default() -> Self {
        Self {
            offsets: [0; 17],
            symbols: [0; 162],
            is_set: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DecodingOutcome {
    None,
    QTableSet,
    StartOfFrame,
    HuffmanTable,
    StartOfScan,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JPEGHeader {
    jfif: Option<APP0>,
    qtables: [QTable; 4],
    restart_interval: u16,
    zero_based_component_id: bool,
    huffman_tables_dc: [HuffmanTable; 4],
    huffman_tables_ac: [HuffmanTable; 4],
    components: [ColorComponent; 4],
    is_sof_set: bool,
    height: u16,
    width: u16,
    start_of_selection: u8,
    end_of_selection: u8,
    successive_approximation_high: u8,
    successive_approximation_low: u8,
    huffman_data: Vec<u8>,
}

impl Default for JPEGHeader {
    fn default() -> Self {
        Self {
            jfif: None,
            qtables: [QTable::default(); 4],
            restart_interval: 0,
            zero_based_component_id: false,
            huffman_tables_dc: [HuffmanTable::default(); 4],
            huffman_tables_ac: [HuffmanTable::default(); 4],
            components: [ColorComponent::default(); 4],
            is_sof_set: false,
            height: 0,
            width: 0,
            start_of_selection: 0,
            end_of_selection: 63,
            successive_approximation_low: 0,
            successive_approximation_high: 0,
            huffman_data: Vec::default(),
        }
    }
}

impl JPEGHeader {
    pub fn new(stream: Vec<u8>) -> Result<JPEGHeader> {
        let mut stream = stream.into_iter();

        let mut has_soi = false;
        let mut has_sof = false;
        let mut has_qtable = false;
        let mut has_htable = false;
        let mut has_sos = false;

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

        let mut jpeg_header = JPEGHeader::default();

        // Advance until next marker
        while let Some(byte) = stream.next() {
            if byte == 0xFF {
                if stream.peek().is_some() {
                    match Marker::read(&mut stream, &mut jpeg_header)? {
                        DecodingOutcome::StartOfFrame => {
                            has_sof = true;
                        }
                        DecodingOutcome::QTableSet => {
                            has_qtable = true;
                        }
                        DecodingOutcome::HuffmanTable => {
                            has_htable = true;
                        }
                        DecodingOutcome::StartOfScan => {
                            has_sos = true;
                            break;
                        }
                        DecodingOutcome::None => {}
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

        if !has_htable {
            return Err(Error::HTableNotFound);
        }

        if !has_sos {
            return Err(Error::SOSNotFound);
        }

        Marker::scan(&mut stream, &mut jpeg_header)?;

        // Last validations
        for component in jpeg_header.components.iter() {
            if (component.is_used_sos && !component.is_used_sof)
                || (component.is_used_sof && !component.is_used_sos)
            {
                return Err(Error::InvalidColorComponent);
            }

            match jpeg_header
                .huffman_tables_dc
                .get(component.huffman_table_dc_id as usize)
            {
                Some(htable) => {
                    if !htable.is_set {
                        return Err(Error::InvalidColorComponent);
                    }
                }
                None => return Err(Error::InvalidColorComponent),
            }

            match jpeg_header
                .huffman_tables_ac
                .get(component.huffman_table_ac_id as usize)
            {
                Some(htable) if !htable.is_set => return Err(Error::InvalidColorComponent),
                None => return Err(Error::InvalidColorComponent),
                _ => {}
            }

            match jpeg_header.qtables.get(component.qtable as usize) {
                Some(qtable) => {
                    if !qtable.is_set {
                        return Err(Error::InvalidColorComponent);
                    }
                }
                None => return Err(Error::InvalidColorComponent),
            }
        }

        Ok(jpeg_header)
    }
}
