use crc::Crc;
use defmt::error;

use crate::{commands::Packet, errors::PacketError};

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
}

mod indexes {
    // From the start
    pub const S_ACTION: usize = 1;
    pub const S_PAYLOAD_LEN: usize = 5;
    pub const S_PAYLOAD_START: usize = 6;

    // From the end
    pub const E_CHECKSUM: usize = 1;
    pub const E_PAYLOAD_END: usize = 2;
}

const SERVICE_FIELDS_LEN: usize = 8;

pub struct PacketParser {
    crc: Crc<u8>,
}

impl PacketParser {
    fn checksum(&self, packet: &[u8]) -> u8 {
        self.crc.checksum(packet)
    }

    pub fn incoming<'a>(&self, buffer: &'a [u8]) -> Result<Packet<'a>, PacketError> {
        if buffer.len() < SERVICE_FIELDS_LEN {
            return Err(PacketError::Len);
        }

        let payload_len = buffer[indexes::S_PAYLOAD_LEN] as usize;
        let proposed_len = SERVICE_FIELDS_LEN + payload_len;
        let end = proposed_len - 1;
        let payload_end = end - indexes::E_PAYLOAD_END;

        if proposed_len > buffer.len() {
            error!("misformed packet - payload len is {}", payload_len);
            return Err(PacketError::Len);
        }

        if buffer[0] != magic::START_SENTINEL || buffer[end] != magic::END_SENTINEL {
            return Err(PacketError::Format);
        }

        let their_checksum = buffer[end - indexes::E_CHECKSUM];
        let our_checksum = self.checksum(&buffer[indexes::S_ACTION..=payload_end]);

        if their_checksum == our_checksum {
            let payload = &buffer[indexes::S_PAYLOAD_START..=payload_end];
            let action = buffer[indexes::S_ACTION];

            Ok(Packet { action, payload })
        } else {
            error!("crc mismatch - {} vs {}", their_checksum, our_checksum);
            Err(PacketError::Checksum)
        }
    }

    pub fn outgoing(&self, buffer: &mut [u8], data: &Packet) -> Result<usize, PacketError> {
        let payload_len = data.payload.len();
        let proposed_len = payload_len + SERVICE_FIELDS_LEN;
        let end = proposed_len - 1;
        let payload_end = end - indexes::E_PAYLOAD_END;

        if proposed_len > buffer.len() || payload_len > u8::MAX as usize {
            return Err(PacketError::Len);
        }

        buffer.fill(0);

        buffer[0] = magic::START_SENTINEL;
        buffer[indexes::S_ACTION] = data.action;
        buffer[indexes::S_PAYLOAD_LEN] = payload_len as u8;

        buffer[indexes::S_PAYLOAD_START..=payload_end].copy_from_slice(data.payload);
        buffer[end] = magic::END_SENTINEL;

        // Compute the checksum and fill in the packet
        let checksum = self.checksum(&buffer[indexes::S_ACTION..=payload_end]);
        buffer[end - indexes::E_CHECKSUM] = checksum;

        Ok(proposed_len)
    }

    pub fn new() -> Self {
        Self {
            // have nothing to do with SMBUS, just appears to be matching their algo
            crc: Crc::<u8>::new(&crc::CRC_8_SMBUS),
        }
    }
}
