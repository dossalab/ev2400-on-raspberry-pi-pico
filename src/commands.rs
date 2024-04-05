use defmt::{error, info};

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

#[derive(defmt::Format)]
pub struct Packet<'a> {
    pub action: u8,
    pub payload: &'a [u8],
}

impl<'a> Packet<'a> {
    pub const fn new(action: u8, payload: &'a [u8]) -> Self {
        Self { action, payload }
    }

    // 2 more existing codes:
    // 0x94 = timeout or device busy?
    // 0x95 = not able to find a free comm adapter
    pub const fn response_error(e: ResponseError) -> Self {
        match e {
            ResponseError::Other => Self::new(responses::ERR, &[0, 0x90]),
            ResponseError::BadChecksum => Self::new(responses::ERR, &[0, 0x91]),
            ResponseError::TimeoutBusy => Self::new(responses::ERR, &[0, 0x92]),
            ResponseError::NoAck => Self::new(responses::ERR, &[0, 0x93]),
        }
    }

    pub const fn response_status() -> Self {
        // This actually contains the version of our 'probe' :)
        Self::new(responses::STATUS, &[FW_VERSION_MAJOR, FW_VERSION_MINOR])
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
            return Some(Packet::response_status());
        }

        _ => error!("unknown command - {}", command),
    }

    // Just pretend we don't have such device if we're asked about any other bus :)
    Some(Packet::response_error(ResponseError::NoAck))
}
