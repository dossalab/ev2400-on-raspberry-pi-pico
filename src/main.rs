#![no_std]
#![no_main]
#![feature(future_join)]

mod descriptors;

use crate::descriptors::TiHidReport;
use assign_resources::assign_resources;
use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, gpio, peripherals, uart, usb};
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid;
use futures::future::join;
use git_version::git_version;
use static_cell::StaticCell;
use usbd_hid::descriptor::SerializedDescriptor;

use defmt::{info, unwrap};
use panic_probe as _;

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

async fn handle_hid<'a, D>(hid: &mut HidReaderWriter2<'a, D>)
where
    D: embassy_usb_driver::Driver<'a>,
{
    let mut command_buffer: [u8; 64] = [0; 64];
    let mut response_buffer: [u8; 62] = [0; 62];

    loop {
        info!("waiting for the HID messages...");
        hid.read(&mut command_buffer).await;

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

        hid.write(&response).await;
    }
}

#[embassy_executor::task]
async fn blinky(res: LedResources) {
    let mut led = gpio::Output::new(res.led, gpio::Level::Low);

    loop {
        info!("running!...");
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

    join(handle_hid(&mut hid), usb.run()).await;
}
