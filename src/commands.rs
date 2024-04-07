use defmt::{error, info, warn};

use crate::parser::Packet;

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

pub fn process_message(command: Packet) -> Option<Packet> {
    match command.action {
        requests::I2C_READ => {
            static I2C_BUF: [u8; 32] = [0; 32];

            info!("i2c read block {}", command);

            let howmuch = command.payload[2] as usize;
            let answer_len = howmuch + 2;

            if answer_len > I2C_BUF.len() {
                error!("they request too much i2c data");
            } else {
                let response = &I2C_BUF[0..answer_len];
                return Some(Packet::new(responses::I2C_READ, response));
            }
        }

        requests::I2C_WRITE => {
            info!("i2c write block {}", command);

            // seems ok not to answer here, in fact if we answer it will confuse the host
            return None;
        }

        requests::STATUS => {
            info!("device status");

            // We can also return error here if we're still busy or something like that....
            return Some(PacketFactory::response_status());
        }

        _ => warn!("unknown command - {}", command),
    }

    // Just pretend we don't have such device if we're asked about any other bus :)
    Some(PacketFactory::response_error(ResponseError::NoAck))
}
