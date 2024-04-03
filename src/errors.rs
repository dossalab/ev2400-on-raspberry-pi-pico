use embassy_usb::class::hid::ReadError;
use embassy_usb_driver::EndpointError;

#[derive(defmt::Format)]
pub enum PacketError {
    Len,
    Format,
    Checksum,
}

#[derive(defmt::Format)]
pub enum HandleError {
    HidReadError(ReadError),
    HidWriteError(EndpointError),
    PacketError(PacketError),
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

impl From<PacketError> for HandleError {
    fn from(err: PacketError) -> Self {
        HandleError::PacketError(err)
    }
}
