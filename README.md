# Embedded Audio

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

# Early Development State


> ⚠️ **Warning: Early Development**
> 
> This project is in its early stages of development and is currently **NOT USABLE**. 

A `no_std` audio processing framework for embedded systems, inspired by ESP-ADF (Espressif Audio Development Framework). 

## Planned Features

The framework aims to provide:

- Audio element pipeline system
- Both async and no_async
- Support for various input streams (I2S, FileSystem, ADC ...)
- Multiple output streams (I2S, DAC ...)
- Audio format decoder and encoder
- Flexible audio processing chain
- Can be test on std platform