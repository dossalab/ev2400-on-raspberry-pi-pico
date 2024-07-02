## Quick & dirty EV2400 reimplementation using RP2040 (Raspberry Pi Pico)

This project allows you to convert your Raspberry Pi Pico to an EV2400 - a hardware interface device from Texas Instruments, in theory allowing you to interface with battery gauge ICs and other hardware from that manufacturer.

This is a limited and not very compliant implementation - since there's no official documentation on the wire format some things have to be guessed. Your mileage may vary - and adding to it, some gauges may become damaged if incorrect values are used! Use at your own risk.

Only I2C and GPIOs are supported at the moment. It should be trivial to add support for SMBus as well. This was so far tested with BQ27427, but I assume other similar I2C chips will work just fine.

## Usage

Get the latest uf2 firmware from the 'Releases' tab.
Press the 'bootsel' button while plugging in your pico, then drag & drop the provided file to the storage device detected.

Your computer should now detect a new HID device called EV2400. Then you may try to attach your gauge and run the Battery Studio. If everything is attached correctly it shall detect both the interface and your target hardware.

## Pin mapping

Pico uses 3.3V logic levels - if your hardware uses something different you will need a voltage level shifter.

| Pico pin | GP | Function |
|----|----|---------|
| 11 |  8 | I2C SDA |
| 12 |  9 | I2C SCL |
| 14 | 10 | GPIO output 1 |
| 15 | 11 | GPIO output 2 |
| 16 | 12 | GPIO output 4 |
|  1 |  0 | Defmt logger (this firmware debugging) |

## Development

I use BlackMagic probe for debugging and writing the firmware to the target. There are some supporting scripts to help write binaries to the target hooked to `cargo run`. For creating UF2 files [this tool](https://github.com/JoNil/elf2uf2-rs) is used (available in ArchLinux repository or through `cargo install`)
Firmware uses serial for `defmt` log output. That was done to help debug USB issues (you can unplug and plug back USB without losing early logs). You will need to compile your own firmware if you want to see these (because `defmt` indexes logs and uses original ELF firmware for lookup)
