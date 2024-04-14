use defmt::{error, info, trace};
use embassy_rp::{
    gpio::{self},
    i2c, peripherals,
};

use embedded_hal_async::i2c::I2c;

use crate::{parser::Packet, usb::USB_ENDPOINT_SIZE, CommResources, Irqs};

mod requests {
    pub const I2C_READ: u8 = 0x1d;
    pub const I2C_WRITE: u8 = 0x1e;
    pub const STATUS: u8 = 0x80;
    pub const GPIO_WRITE: u8 = 0xb7;
}

mod responses {
    pub const I2C_READ: u8 = 0x52;
    pub const STATUS: u8 = 0xc0;
    pub const ERR: u8 = 0x46;
}

mod outputs {
    pub const PIN1: u32 = 0x40;
    pub const PIN2: u32 = 0x80;
    pub const PIN4: u32 = 0x8000;
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

pub struct Communicator {
    i2c_buffer: [u8; I2C_BUFFER_SIZE],
    i2c: i2c::I2c<'static, peripherals::I2C0, i2c::Async>,

    out1: gpio::Output<'static>,
    out2: gpio::Output<'static>,
    out4: gpio::Output<'static>,
}

impl Communicator {
    async fn i2c_read<'a>(&mut self, addr: u8, start: u8, len: usize) -> Option<Packet> {
        info!("i2c: reading, start is {:x}, len is {:x}", start, len);

        // 2 bytes are reserved for some metadata...
        let payload = &mut self.i2c_buffer[0..len + 2];

        // though this does not seem to affect anything...
        payload[0] = 0;
        payload[1] = 0;

        match self.i2c.write_read(addr, &[start], &mut payload[2..]).await {
            Ok(_) => Some(Packet::new(responses::I2C_READ, payload)),
            Err(_) => Some(PacketFactory::response_error(ResponseError::NoAck)),
        }
    }

    async fn i2c_write<'a>(&mut self, addr: u8, start: u8, data: &[u8]) -> Option<Packet> {
        info!(
            "i2c: writing, start is {:x}, data is {:x} ({} bytes)",
            start,
            data,
            data.len()
        );

        // we reserve 1 byte for the 'start' argument
        let transaction_len = data.len() + 1;
        let buffer = &mut self.i2c_buffer[0..transaction_len];

        // it's ugly that we have to copy, after doing all that... but doing 2 i2c writes ain't going to cut it...
        buffer[0] = start;
        buffer[1..].copy_from_slice(data);

        match self.i2c.write(addr, buffer).await {
            Err(_) => Some(PacketFactory::response_error(ResponseError::NoAck)),

            // it's expected not to send any response here - in fact if we send something it will confuse the host
            Ok(_) => None,
        }
    }

    async fn i2c_op<'a>(&mut self, command: &Packet<'a>, read: bool) -> Option<Packet> {
        let bad_packet_response = Some(PacketFactory::response_error(ResponseError::Other));

        if command.payload.len() < 3 {
            error!("bad i2c packet - missing fields in the payload");
            return bad_packet_response;
        }

        let address = command.payload[0] >> 1;
        let start = command.payload[1];
        let len = command.payload[2] as usize;

        trace!(
            "i2c: address is {}, starting at {}, data len is {}",
            address,
            start,
            len
        );

        if read {
            // 2 fields are reserved for response metadata
            let payload_len = 2 + len;

            if payload_len > Packet::max_payload(USB_ENDPOINT_SIZE) {
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

    fn gpio_write(&mut self, command: &Packet) -> Option<Packet> {
        let data = command.payload;

        if data.len() != 7 {
            error!("bad gpio write packet (len is {})", data.len());
            return Some(PacketFactory::response_error(ResponseError::Other));
        }

        // Ti sure had lots of fun designing this format
        let mask = u32::from_be_bytes([0, data[2], data[4], data[6]]);
        let port = u32::from_be_bytes([0, data[1], data[3], data[5]]);

        trace!("gpio write: mask is {:x}, port is {:x}", mask, port);

        let set_output = |out: &mut gpio::Output, pin: u32, mask: u32, port: u32| {
            if mask & pin > 0 {
                if port & pin > 0 {
                    out.set_high()
                } else {
                    out.set_low()
                }
            }
        };

        set_output(&mut self.out1, outputs::PIN1, mask, port);
        set_output(&mut self.out2, outputs::PIN2, mask, port);
        set_output(&mut self.out4, outputs::PIN4, mask, port);

        // it's not clear whether we should return some status or not
        None
    }

    // Waiting is a problem here... we most likely want to report back and THEN
    // do the i2c work instead of doing everything step-by step
    // Most transactions at this stage are pretty small so it's probably not that bad...
    pub async fn run<'a>(&mut self, request: &Packet<'a>) -> Option<Packet> {
        match request.action {
            requests::I2C_READ => self.i2c_op(request, true).await,
            requests::I2C_WRITE => self.i2c_op(request, false).await,
            requests::GPIO_WRITE => self.gpio_write(request),

            // So we're never busy - if we received that message, we're not doing any i2c work
            requests::STATUS => Some(PacketFactory::response_status()),

            _ => {
                trace!("unhandled packet: {}", request);
                Some(PacketFactory::response_error(ResponseError::NoAck))
            }
        }
    }

    pub fn new(r: CommResources) -> Self {
        let gpio_state = gpio::Level::Low;

        Self {
            i2c: i2c::I2c::new_async(r.i2c, r.scl, r.sda, Irqs, i2c::Config::default()),
            i2c_buffer: [0; I2C_BUFFER_SIZE],

            out1: gpio::Output::new(r.pin_1, gpio_state),
            out2: gpio::Output::new(r.pin_2, gpio_state),
            out4: gpio::Output::new(r.pin_4, gpio_state),
        }
    }
}
