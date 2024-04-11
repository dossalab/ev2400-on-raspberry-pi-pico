use defmt::info;
use defmt::{error, trace};
use embassy_rp::{i2c, usb};
use embassy_usb::class::hid::{self, HidReaderWriter};
use embassy_usb_driver as usbhw;
use usbd_hid::descriptor::gen_hid_descriptor;
use usbd_hid::descriptor::generator_prelude::*;

use crate::commands::Communicator;
use crate::indications::LedIndications;
use crate::parser::{PacketParser, ParserError};
use crate::{I2cResources, Irqs, UsbResources};

use futures::future::join;

// Basic USB parameters
const USB_VID: u16 = 0x451;
const USB_PID: u16 = 0x37;
const USB_PACKET_SIZE: usize = 64;
const USB_MANUFACTURER: &str = "Texas Instruments";
const USB_PRODUCT_NAME: &str = "EV2400";
const USB_SERIAL_NUMBER: &str = "0000000008000000";

// TODO: why can't we use this in the following macro?
pub const USB_ENDPOINT_SIZE: usize = 62;

#[gen_hid_descriptor(
     (collection = APPLICATION, usage_page = 0xFF09, usage = 1) = {
        (report_id = 1, usage = 1) = {
            buff1=input
        };
        (report_id = 0x3F, usage = 1) = {
            buff2=output
        };
     }
)]
#[allow(dead_code)]
pub struct TiHidReport {
    pub buff1: [u8; 62],
    pub buff2: [u8; 62],
}

#[derive(defmt::Format)]
enum HandleError {
    HidReadError(hid::ReadError),
    HidWriteError(usbhw::EndpointError),
    ParserError(ParserError),
}

impl From<hid::ReadError> for HandleError {
    fn from(err: hid::ReadError) -> Self {
        HandleError::HidReadError(err)
    }
}

impl From<usbhw::EndpointError> for HandleError {
    fn from(err: usbhw::EndpointError) -> Self {
        HandleError::HidWriteError(err)
    }
}

impl From<ParserError> for HandleError {
    fn from(err: ParserError) -> Self {
        HandleError::ParserError(err)
    }
}

struct HidHandler<'a, U, I>
where
    U: embassy_usb_driver::Driver<'a>,
    I: embedded_hal_async::i2c::I2c,
{
    parser: PacketParser,
    hid: HidReaderWriter<'a, U, USB_PACKET_SIZE, USB_PACKET_SIZE>,
    indications: &'a LedIndications,
    comm: Communicator<I>,

    // TODO: why the out buffer has to be one byte smaller than the packet size?
    in_buffer: [u8; USB_PACKET_SIZE],
    out_buffer: [u8; USB_PACKET_SIZE - 1],
}

impl<'a, U, I> HidHandler<'a, U, I>
where
    U: embassy_usb_driver::Driver<'a>,
    I: embedded_hal_async::i2c::I2c,
{
    pub async fn run(&mut self) -> Result<(), HandleError> {
        self.hid.read(&mut self.in_buffer).await?;

        self.indications.signal(());
        let request = self.parser.incoming(&self.in_buffer[2..])?;

        match self.comm.run(&request).await {
            Some(response) => {
                let len = self.parser.outgoing(&mut self.out_buffer[2..], &response)?;

                self.out_buffer[0] = 0x01;
                self.out_buffer[1] = len as u8;

                self.hid.write(&self.out_buffer).await?;
            }

            _ => {}
        }

        Ok(())
    }

    fn new(
        comm: Communicator<I>,
        parser: PacketParser,
        hid: HidReaderWriter<'a, U, USB_PACKET_SIZE, USB_PACKET_SIZE>,
        indications: &'a LedIndications,
    ) -> Self {
        Self {
            parser,
            hid,
            indications,
            comm,
            in_buffer: [0; USB_PACKET_SIZE],
            out_buffer: [0; USB_PACKET_SIZE - 1],
        }
    }
}

#[embassy_executor::task]
pub async fn run(usbr: UsbResources, i2cr: I2cResources, indications: &'static LedIndications) {
    let driver = usb::Driver::new(usbr.usb, Irqs);
    let mut usb_config = embassy_usb::Config::new(USB_VID, USB_PID);

    usb_config.manufacturer = Some(USB_MANUFACTURER);
    usb_config.product = Some(USB_PRODUCT_NAME);
    usb_config.serial_number = Some(USB_SERIAL_NUMBER);

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut state = hid::State::new();

    let mut builder = embassy_usb::Builder::new(
        driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    let hid_config = hid::Config {
        report_descriptor: &TiHidReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: USB_PACKET_SIZE as u16,
    };

    let hid = HidReaderWriter::new(&mut builder, &mut state, hid_config);
    let mut usb = builder.build();
    let i2c = i2c::I2c::new_async(i2cr.i2c, i2cr.scl, i2cr.sda, Irqs, i2c::Config::default());

    join(
        async {
            let comm = Communicator::new(i2c);
            let parser = PacketParser::new();
            let mut handler = HidHandler::new(comm, parser, hid, indications);

            loop {
                trace!("handling HID events...");

                if let Err(e) = handler.run().await {
                    // actually speaking not all errors require such harsh report message
                    // i'd prefer to keep those for HW errors only, as parser errors are unfortunately pretty common...
                    error!("error while handling HID operations - {}", e);
                }
            }
        },
        usb.run(),
    )
    .await;
}
