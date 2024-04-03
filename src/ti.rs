use crc::Crc;
use defmt::error;

use crate::errors::PacketError;

// Packet format is as follows:
//
// [0] = 0xAA (start sequence)
// ----
// [1] = command or status
// [2]
// [3]
// [4]
// [5] = payload length
// ----
// [ ] = payload
// [ ]
// [ ]
// ----
// [end - 1] = CRC8
// [end] = 0x55 (end sentinel)

mod magic {
    pub const START_SENTINEL: u8 = 0xAA;
    pub const END_SENTINEL: u8 = 0x55;
    pub const CHECKSUM_PLACEHOLDER: u8 = 0x00;
}

mod indexes {
    // From the start
    pub const S_ACTION: usize = 1;
    pub const S_PAYLOAD_LEN: usize = 5;
    pub const S_PAYLOAD_START: usize = 6;

    // From the end
    pub const E_CHECKSUM: usize = 1;
}

const SERVICE_FIELDS_LEN: usize = 8;

pub struct PacketParser {
    crc: Crc<u8>,
}

impl PacketParser {
    fn checksum(&self, packet: &[u8]) -> u8 {
        self.crc.checksum(packet)
    }

    pub fn parse<'a>(&self, buffer: &'a mut [u8]) -> Result<(u8, &'a [u8]), PacketError> {
        if buffer.len() < SERVICE_FIELDS_LEN {
            return Err(PacketError::Len);
        }

        let payload_len = buffer[indexes::S_PAYLOAD_LEN] as usize;
        let proposed_len = SERVICE_FIELDS_LEN + payload_len;
        let start = 0;
        let end = proposed_len - 1;

        if proposed_len > buffer.len() {
            error!("misformed packet - payload len is {}", payload_len);
            return Err(PacketError::Len);
        }

        if buffer[start] != magic::START_SENTINEL || buffer[end] != magic::END_SENTINEL {
            return Err(PacketError::Format);
        }

        let their_checksum = buffer[end - indexes::E_CHECKSUM];

        // Remove the provided checksum and calculate ours
        buffer[end - indexes::E_CHECKSUM] = magic::CHECKSUM_PLACEHOLDER;
        let our_checksum = self.checksum(&buffer[start + 1..end - 1]);

        if their_checksum == our_checksum {
            let payload = &buffer[indexes::S_PAYLOAD_START..indexes::S_PAYLOAD_START + payload_len];
            let action = buffer[indexes::S_ACTION];

            Ok((action, payload))
        } else {
            Err(PacketError::Checksum)
        }
    }

    pub fn new() -> Self {
        Self {
            // have nothing to do with SMBUS, just appears to be matching their algo
            crc: Crc::<u8>::new(&crc::CRC_8_SMBUS),
        }
    }
}
