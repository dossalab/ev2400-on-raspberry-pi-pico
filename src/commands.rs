use defmt::{error, info, trace};

use crate::{parser::Packet, usb::USB_ENDPOINT_SIZE};

mod requests {
    pub const I2C_READ: u8 = 0x1d;
    pub const I2C_WRITE: u8 = 0x1e;
    pub const STATUS: u8 = 0x80;
}

mod responses {
    pub const I2C_READ: u8 = 0x52;
    pub const STATUS: u8 = 0xc0;
    pub const ERR: u8 = 0x46;
}

const FW_VERSION_MAJOR: u8 = 6;
const FW_VERSION_MINOR: u8 = 66;

#[allow(dead_code)]
pub enum ResponseError {
    BadChecksum,
    TimeoutBusy,
    NoAck,

    // This doesn't seem to be supported in any way
    Other,
}

struct PacketFactory;

impl PacketFactory {
    pub const fn response_status<'a>() -> Packet<'a> {
        // This actually contains the version of our 'probe' :)
        Packet::new(responses::STATUS, &[FW_VERSION_MAJOR, FW_VERSION_MINOR])
    }

    // 2 more existing codes:
    // 0x94 = timeout or device busy?
    // 0x95 = not able to find a free comm adapter
    pub const fn response_error<'a>(e: ResponseError) -> Packet<'a> {
        match e {
            ResponseError::Other => Packet::new(responses::ERR, &[0, 0x90]),
            ResponseError::BadChecksum => Packet::new(responses::ERR, &[0, 0x91]),
            ResponseError::TimeoutBusy => Packet::new(responses::ERR, &[0, 0x92]),
            ResponseError::NoAck => Packet::new(responses::ERR, &[0, 0x93]),
        }
    }
}

const I2C_BUFFER_SIZE: usize = 64;

pub struct Communicator<I: embedded_hal_async::i2c::I2c> {
    i2c_buffer: [u8; I2C_BUFFER_SIZE],
    i2c: I,
}

impl<I> Communicator<I>
where
    I: embedded_hal_async::i2c::I2c,
{
    async fn i2c_read<'a>(&mut self, address: u8, start: u8, len: usize) -> Option<Packet> {
        info!("i2c: reading...");

        let buffer = &mut self.i2c_buffer[0..len];

        match self.i2c.write_read(address, &[start], buffer).await {
            Ok(_) => Some(PacketFactory::response_error(ResponseError::NoAck)),
            Err(_) => Some(Packet::new(responses::I2C_READ, buffer)),
        }
    }

    async fn i2c_write<'a>(&mut self, address: u8, start: u8, data: &[u8]) -> Option<Packet> {
        info!("i2c: writing...");

        let r1 = self.i2c.write(address, &[start]).await;
        let r2 = self.i2c.write(address, data).await;

        if r1.is_err() || r2.is_err() {
            Some(PacketFactory::response_error(ResponseError::NoAck))
        } else {
            // it's expected not to send any response here - in fact if we send something it will confuse the host
            None
        }
    }

    async fn i2c_op<'a>(&mut self, command: &Packet<'a>, read: bool) -> Option<Packet> {
        let bad_packet_response = Some(PacketFactory::response_error(ResponseError::Other));

        if command.payload.len() < 3 {
            error!("bad i2c packet - missing fields in the payload");
            return bad_packet_response;
        }

        let address = command.payload[0];
        let start = command.payload[1];
        let len = command.payload[2] as usize;

        trace!(
            "i2c: address is {}, starting at {}, data len is {}",
            address,
            start,
            len
        );

        if read {
            if len > Packet::max_payload(USB_ENDPOINT_SIZE) || len > self.i2c_buffer.len() {
                error!(
                    "bad packet - can't send / hold that much data (requested len is {})",
                    len
                );

                bad_packet_response
            } else {
                self.i2c_read(address, start, len).await
            }
        } else {
            let i2c_data = &command.payload[3..];

            if len != i2c_data.len() {
                error!(
                    "bad packet - length (given is {}, actual is {})",
                    len,
                    i2c_data.len()
                );

                bad_packet_response
            } else {
                self.i2c_write(address, start, i2c_data).await
            }
        }
    }

    // Waiting is a problem here... we most likely want to report back and THEN
    // do the i2c work instead of doing everything step-by step
    // Most transactions at this stage are pretty small so it's probably not that bad...
    pub async fn run<'a>(&mut self, request: &Packet<'a>) -> Option<Packet> {
        match request.action {
            requests::I2C_READ => self.i2c_op(request, true).await,
            requests::I2C_WRITE => self.i2c_op(request, false).await,

            // So we're never busy - if we received that message, we're not doing any i2c work
            requests::STATUS => Some(PacketFactory::response_status()),

            _ => Some(PacketFactory::response_error(ResponseError::NoAck)),
        }
    }

    pub fn new(i2c: I) -> Self {
        Self {
            i2c,
            i2c_buffer: [0; I2C_BUFFER_SIZE],
        }
    }
}
