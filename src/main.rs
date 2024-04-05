#![no_std]
#![no_main]
#![feature(future_join)]

mod commands;
mod descriptors;
mod errors;
mod parser;

use crate::{descriptors::TiHidReport, errors::PacketError};

use commands::{Packet, ResponseError};
use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, gpio, peripherals, uart, usb};
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid;
use errors::HandleError;
use futures::future::join;
use git_version::git_version;
use parser::PacketParser;
use static_cell::StaticCell;
use usbd_hid::descriptor::SerializedDescriptor;

use defmt::{error, info, trace, unwrap};
use panic_probe as _;

use assign_resources::assign_resources;

// Basic USB parameters
const USB_VID: u16 = 0x451;
const USB_PID: u16 = 0x37;
const USB_PACKET_SIZE: usize = 64;
const USB_MANUFACTURER: &str = "Texas Instruments";
const USB_PRODUCT_NAME: &str = "EV2400";
const USB_SERIAL_NUMBER: &str = "F7BA1B5108002500";

type HidReaderWriter2<'a, D> = hid::HidReaderWriter<'a, D, USB_PACKET_SIZE, USB_PACKET_SIZE>;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<peripherals::USB>;
});

assign_resources! {
    led: LedResources {
        led: PIN_25
    },
    logger: LoggerResources {
        uart: UART0,
        tx: PIN_0,
        rx: PIN_1
    }
}

async fn communicate<'a, D>(
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

    let response = match parser.incoming(&in_buffer[2..]) {
        Ok(request) => commands::process_message(request),
        Err(error) => {
            let response = match error {
                PacketError::Checksum => Packet::response_error(ResponseError::BadChecksum),
                _ => Packet::response_error(ResponseError::Other),
            };

            Some(response)
        }
    };

    if let Some(r) = response {
        let len = parser.outgoing(&mut out_buffer[2..], &r)?;

        out_buffer[0] = 0x01;
        out_buffer[1] = len as u8;

        hid.write(&out_buffer).await?;
    }

    Ok(())
}

#[embassy_executor::task]
async fn blinky(res: LedResources) {
    let mut led = gpio::Output::new(res.led, gpio::Level::Low);

    loop {
        // info!("running!...");
        led.toggle();
        Timer::after(Duration::from_secs(1)).await;
    }
}

fn setup_uart_logger(res: LoggerResources) {
    let config = uart::Config::default();
    let uart = uart::Uart::new_blocking(res.uart, res.tx, res.rx, config);

    static SERIAL: StaticCell<uart::Uart<'_, peripherals::UART0, uart::Blocking>> =
        StaticCell::new();
    let serial = SERIAL.init(uart);

    defmt_serial::defmt_serial(serial);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let embassy = embassy_rp::init(Default::default());
    let resources = split_resources!(embassy);

    setup_uart_logger(resources.logger);

    info!("ti-i2c ({}) is running. Hello!", git_version!());

    unwrap!(spawner.spawn(blinky(resources.led)));

    let driver = usb::Driver::new(embassy.USB, Irqs);
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

                if let Err(e) = communicate(&parser, &mut hid).await {
                    error!("error while handling HID operations - {}", e);
                }
            }
        },
        // This could be a separate task but it's pretty hard to make the lifetimes happy in that case :)
        usb.run(),
    )
    .await;
}
