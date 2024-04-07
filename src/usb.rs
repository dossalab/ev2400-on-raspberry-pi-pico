use defmt::error;
use defmt::trace;
use embassy_rp::usb;
use embassy_usb::class::hid;
use embassy_usb_driver as usbhw;
use futures::future::join;
use usbd_hid::descriptor::gen_hid_descriptor;
use usbd_hid::descriptor::generator_prelude::*;

use crate::commands;
use crate::indications::LedIndications;
use crate::parser::PacketParser;
use crate::parser::ParserError;
use crate::Irqs;
use crate::UsbResources;

// Basic USB parameters
const USB_VID: u16 = 0x451;
const USB_PID: u16 = 0x37;
const USB_PACKET_SIZE: usize = 64;
const USB_MANUFACTURER: &str = "Texas Instruments";
const USB_PRODUCT_NAME: &str = "EV2400";
const USB_SERIAL_NUMBER: &str = "0000000008000000";

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

type HidReaderWriter2<'a, D> = hid::HidReaderWriter<'a, D, USB_PACKET_SIZE, USB_PACKET_SIZE>;

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

async fn communicate<'a, D>(
    indications: &LedIndications,
    parser: &PacketParser,
    hid: &mut HidReaderWriter2<'a, D>,
) -> Result<(), HandleError>
where
    D: embassy_usb_driver::Driver<'a>,
{
    // TODO: why the out buffer has to be one byte smaller than the packet size?
    let mut in_buffer = [0; USB_PACKET_SIZE];
    let mut out_buffer = [0; USB_PACKET_SIZE - 1];

    hid.read(&mut in_buffer).await?;
    let request = parser.incoming(&in_buffer[2..])?;

    indications.signal(());

    match commands::process_message(request) {
        Some(response) => {
            let len = parser.outgoing(&mut out_buffer[2..], &response)?;

            out_buffer[0] = 0x01;
            out_buffer[1] = len as u8;

            hid.write(&out_buffer).await?;
        }

        None => {}
    }

    Ok(())
}

#[embassy_executor::task]
pub async fn run(r: UsbResources, indications: &'static LedIndications) {
    let driver = usb::Driver::new(r.usb, Irqs);
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

    let mut hid = HidReaderWriter2::new(&mut builder, &mut state, hid_config);
    let mut usb = builder.build();

    join(
        async {
            let parser = PacketParser::new();
            loop {
                trace!("handling HID events...");

                if let Err(e) = communicate(indications, &parser, &mut hid).await {
                    error!("error while handling HID operations - {}", e);
                }
            }
        },
        // This could be a separate task but it's pretty hard to make the lifetimes happy in that case :)
        usb.run(),
    )
    .await;
}
