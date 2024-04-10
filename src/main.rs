#![no_std]
#![no_main]
#![feature(future_join)]

mod commands;
mod indications;
mod parser;
mod usb;

use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, i2c, peripherals, uart, usb as usbhw};
use git_version::git_version;
use indications::LedIndications;
use static_cell::StaticCell;

use defmt::{info, unwrap};
use panic_probe as _;

use assign_resources::assign_resources;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usbhw::InterruptHandler<peripherals::USB>;
    I2C0_IRQ => i2c::InterruptHandler<peripherals::I2C0>;
});

assign_resources! {
    led: LedResources {
        green: PIN_25
    },
    logger: LoggerResources {
        uart: UART0,
        tx: PIN_0,
        rx: PIN_1
    },
    usb: UsbResources {
        usb: USB
    },
    i2c: I2cResources {
        i2c: I2C0,
        sda: PIN_8,
        scl: PIN_9,
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

    static LED_INDICATIONS: LedIndications = LedIndications::new();

    unwrap!(spawner.spawn(indications::run(resources.led, &LED_INDICATIONS)));
    unwrap!(spawner.spawn(usb::run(resources.usb, resources.i2c, &LED_INDICATIONS)));
}
