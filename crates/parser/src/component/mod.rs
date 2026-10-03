use crate::{Component, ParseError, ParseLimitKind, Parser, Result};
mod builder;
mod conversion;

use alloc::vec::Vec;
use builder::ComponentBuilder;
use wasmparser::{Encoding, Payload};

impl Parser {
    /// Parse a component-model Wasm file from bytes.
    pub fn parse_component_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<Component> {
        self.parse_component(wasm.as_ref())
    }

    #[cfg(feature = "std")]
    /// Parse a component-model Wasm file from a file. Requires `std` feature.
    pub fn parse_component_file(&self, path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Component> {
        let file = crate::std::fs::File::open(&path)
            .map_err(|e| ParseError::Other(alloc::format!("Error opening file {:?}: {}", path.as_ref(), e)))?;
        self.parse_component_stream(&mut crate::std::io::BufReader::new(file))
    }

    #[cfg(feature = "std")]
    /// Parse a component-model Wasm file from a stream. Requires `std` feature.
    pub fn parse_component_stream(&self, mut stream: impl crate::std::io::Read) -> Result<Component> {
        let mut buffer = Vec::new();
        let mut total_read = 0;
        let mut buffer_offset = 0;
        let mut eof = false;
        let mut parser = wasmparser::Parser::new(0);
        let mut parents = Vec::new();
        let mut builder = ComponentBuilder::default();
        let mut saw_component = false;
        let mut validator = self.validator(Encoding::Component);

        loop {
            match parser.parse(&buffer[buffer_offset..], eof)? {
                wasmparser::Chunk::NeedMoreData(hint) => {
                    if buffer_offset != 0 {
                        buffer.copy_within(buffer_offset.., 0);
                        buffer.truncate(buffer.len() - buffer_offset);
                        buffer_offset = 0;
                    }
                    eof = self.read_more(&mut stream, &mut buffer, hint, &mut total_read)? == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    validate_payload(validator.as_mut(), &payload)?;
                    buffer_offset += consumed;
                    builder.record_declarations(&payload, self.options().limits())?;
                    match payload {
                        Payload::Version { encoding, .. } if !saw_component => {
                            if encoding != Encoding::Component {
                                return Err(ParseError::InvalidEncoding(encoding));
                            }
                            saw_component = true;
                        }
                        Payload::ModuleSection { parser: nested, unchecked_range } => {
                            let len = usize::try_from(unchecked_range.end - unchecked_range.start)
                                .map_err(|_| ParseError::Other("module size is too large".into()))?;
                            while buffer.len() - buffer_offset < len {
                                let remaining = len - (buffer.len() - buffer_offset);
                                if self.read_more(&mut stream, &mut buffer, remaining, &mut total_read)? == 0 {
                                    return Err(ParseError::EndNotReached);
                                }
                            }
                            let bytes = &buffer[buffer_offset..buffer_offset + len];
                            if validator.is_some() {
                                for payload in nested.parse_all(bytes) {
                                    validate_payload(validator.as_mut(), &payload?)?;
                                }
                            }
                            builder.push_module(self.parse_module_bytes(bytes)?);
                            buffer_offset += len;
                        }
                        Payload::ComponentSection { parser: nested, .. } => {
                            parents.push(parser);
                            builder.enter_component();
                            parser = nested;
                        }
                        Payload::End(_) => match parents.pop() {
                            Some(parent) => {
                                builder.leave_component();
                                parser = parent;
                            }
                            None => {
                                if buffer_offset != buffer.len()
                                    || (!eof && self.read_more(&mut stream, &mut buffer, 1, &mut total_read)? != 0)
                                {
                                    return Err(ParseError::Other("trailing bytes after end of component".into()));
                                }
                                return Ok(builder.finish());
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
    }

    fn parse_component(&self, wasm: &[u8]) -> Result<Component> {
        self.options().limits().check(ParseLimitKind::ModuleBytes, wasm.len())?;

        let mut validator = self.validator(Encoding::Component);

        let mut payloads = wasmparser::Parser::new(0).parse_all(wasm);
        let version = payloads.next().transpose()?.ok_or(ParseError::EndNotReached)?;
        validate_payload(validator.as_mut(), &version)?;
        match version {
            Payload::Version { encoding: Encoding::Component, .. } => {}
            Payload::Version { encoding, .. } => return Err(ParseError::InvalidEncoding(encoding)),
            _ => return Err(ParseError::EndNotReached),
        }

        let mut builder = ComponentBuilder::default();
        for payload in payloads {
            let payload = payload?;
            validate_payload(validator.as_mut(), &payload)?;
            builder.record_declarations(&payload, self.options().limits())?;
            match payload {
                Payload::ModuleSection { unchecked_range, .. } => {
                    let start = usize::try_from(unchecked_range.start)
                        .map_err(|_| ParseError::Other("module offset is too large".into()))?;
                    let end = usize::try_from(unchecked_range.end)
                        .map_err(|_| ParseError::Other("module offset is too large".into()))?;
                    let bytes = wasm.get(start..end).ok_or_else(|| ParseError::Other("invalid module range".into()))?;
                    builder.push_module(self.parse_module_bytes(bytes)?);
                }
                Payload::ComponentSection { .. } => builder.enter_component(),
                Payload::End(_) if !builder.parents.is_empty() => builder.leave_component(),
                _ => {}
            }
        }
        Ok(builder.finish())
    }
}

fn validate_payload(validator: Option<&mut crate::validation::Validator>, payload: &Payload<'_>) -> Result<()> {
    #[cfg(feature = "validate")]
    if let Some(validator) = validator {
        // Embedded modules are validated by parse_module_bytes before they are
        // stored. The component validator still needs every payload to track
        // component index spaces, but validating function bodies here as well
        // would repeat the most expensive part of core validation.
        let _ = validator.payload(payload)?;
    }
    #[cfg(not(feature = "validate"))]
    let _ = (validator, payload);
    Ok(())
}
