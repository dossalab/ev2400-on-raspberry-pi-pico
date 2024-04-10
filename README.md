## Quick & dirty EV2400 reimplementation using RP2040 (Raspberry Pi Pico)

This project allows you to convert your Raspberry Pi Pico to a EV2400 - a hardware interface device from Texas Instruments, in theory allowing you to interface with the battery gauge ICs and similar hardware from that manufacturer.

This is a very limited and probably not very compliant implementation - since there's no official documentation of the USB wire format some things have to be eyeballed.

THIS IS NOT AN ACTIVELY SUPPORTED PROJECT - USE AT YOUR OWN RISK. Some gauges may become damaged if incorrect values are used!

Only I2C is supported at the moment. I don't need SMBus / GPIO or whatever else is supported by the original probe - but it should be trivial to add support.
