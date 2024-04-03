#![no_std]
#![no_main]
#![feature(future_join)]

mod descriptors;
mod errors;
mod ti;

use crate::{descriptors::TiHidReport, ti::PacketParser};

use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, gpio, peripherals, uart, usb};
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid;
use errors::HandleError;
use futures::future::join;
use git_version::git_version;
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

async fn handle_hid<'a, D>(
    parser: &PacketParser,
    hid: &mut HidReaderWriter2<'a, D>,
) -> Result<(), HandleError>
where
    D: embassy_usb_driver::Driver<'a>,
{
    let mut command_buffer: [u8; 64] = [0; 64];
    let mut response_buffer: [u8; 62] = [0; 62];

    hid.read(&mut command_buffer).await?;

    let (command, payload) = parser.parse(&mut command_buffer[2..])?;
    match command {
        // System
        0xbd => info!("set usb char {}", payload),
        0x80 => info!("poll status?"),
        0xee => info!("comm clock speed"),

        // I2C
        0x1d => info!("i2c read block {}", payload),
        0x1e => info!("i2c write block {}", payload),

        // SMBus
        0x01 => info!("read SMBus word {}", payload),
        0x02 => info!("read SMBus block {}", payload),
        0x04 => info!("write SMBus word {}", payload),
        0x05 => info!("write SMBus block {}", payload),

        // SPI
        0xd1 => info!("read SPI block {}", payload),
        0xd2 => info!("read SPI conf {}", payload),
        0xd3 => info!("write SPI block {}", payload),
        0xb2 => info!("write SPI conf {}", payload),

        _ => error!("unknown command - {:x}, {}", command, payload),
    }

    let mut response: [u8; 63] = [0; 63];

    response[0] = 0x01;
    response[1] = 8;

    response[2] = 0xAA;
    response[3] = 0x52;
    response[4] = 0;
    response[5] = 0;
    response[6] = 0;
    response[7] = 0; // payload len
    response[8] = 0xEE; // checkdum
    response[9] = 0x55;

    hid.write(&response).await?;

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

                if let Err(e) = handle_hid(&parser, &mut hid).await {
                    error!("error while handling HID operations - {}", e);
                }
            }
        },
        // This could be a separate task but it's pretty hard to make the lifetimes happy in that case :)
        usb.run(),
    )
    .await;
}
