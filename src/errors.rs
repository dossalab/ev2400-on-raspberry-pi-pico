use embassy_usb::class::hid::ReadError;
use embassy_usb_driver::EndpointError;

use crate::parser::ParserError;

#[derive(defmt::Format)]
pub enum HandleError {
    HidReadError(ReadError),
    HidWriteError(EndpointError),
    ParserError(ParserError),
}

impl From<ReadError> for HandleError {
    fn from(err: ReadError) -> Self {
        HandleError::HidReadError(err)
    }
}

impl From<EndpointError> for HandleError {
    fn from(err: EndpointError) -> Self {
        HandleError::HidWriteError(err)
    }
}

impl From<ParserError> for HandleError {
    fn from(err: ParserError) -> Self {
        HandleError::ParserError(err)
    }
}
