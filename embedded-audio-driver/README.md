# Embedded Audio Driver

A `no_std` driver interface crate for embedded audio devices, providing a set of traits for implementing audio drivers in embedded systems.

https://crates.io/crates/embedded-audio

# Embedded Audio Driver

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> ⚠️ **Warning: Early Development State**
> 
> This project is in its extremely early stages of development and is currently **NOT USABLE**. 
> We are still working on the basic trait definitions and architecture design.
> Everything is subject to change significantly.

A `no_std` driver interface crate that defines traits for embedded audio devices. This crate serves as the foundation for the [embedded-audio](https://crates.io/crates/embedded-audio) framework.

## Overview

This crate aims to provide a common set of traits for:

- Audio Input/Output streams
- I2S (Inter-IC Sound) devices
- DAC (Digital-to-Analog Converter)
- Audio decoders

These traits will be used by the `embedded-audio` framework to build a complete audio processing system.
