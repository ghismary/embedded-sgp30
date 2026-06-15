[![crates.io](https://img.shields.io/crates/v/embedded-sgp30.svg)](https://crates.io/crates/embedded-sgp30)
[![License](https://img.shields.io/crates/l/embedded-sgp30.svg)](https://crates.io/crates/embedded-sgp30)
[![Documentation](https://docs.rs/embedded-sgp30/badge.svg)](https://docs.rs/embedded-sgp30)

# embedded-sgp30

This is a platform agnostic Rust driver the SGP30 digital gas sensor using the
[`embedded-hal`] traits.

[`embedded-hal`]: https://github.com/rust-embedded/embedded-hal

## The device

The SGP30 is a gas sensor for indoor air quality applications such as air
purifiers, demand-controlled ventilation or IoT.

It provides detailed information about the air quality, such as TVOC and CO₂eq.
It also gives access to the raw measurement values of ethanol and H₂.

It is addressed via an I²C interface and has a low power consumption.

Here are its measurement ranges:

| Ethanol           | H₂                | TVOC               | CO₂eq                |
| ----------------- | ----------------- | ------------------ | -------------------- |
| 0 ppm to 1000 ppm | 0 ppm to 1000 ppm | 0 ppb to 60000 ppb | 400 ppm to 60000 ppm |

ppm: parts per million. 1 ppm = 1000 ppb (parts per billion)

For more details on the accuracy and the resolution, take a look at the
datasheet link below.

### Documentation:

- [Datasheet](https://sensirion.com/media/documents/984E0DD5/61644B8B/Sensirion_Gas_Sensors_Datasheet_SGP30.pdf)
- [C source code from the manufacturer](https://github.com/Sensirion/embedded-sgp/tree/master/sgp30)

## Features

- [x] Initialize an air quality measure.
- [x] Perform an air quality measure (returns the TVOC and CO₂eq values).
- [x] Get the baseline values of the baseline correction algorithm.
- [x] Set the baseline values of the baseline correction algorithm.
- [x] Set the humidity to provide the on-chip humidity compensation algorithm
  with an absolute humidity value from an external humidity sensor like the
  sht3x.
- [x] Do a software reset.
- [x] Perform a raw signals measure (returns the Ethanol and H₂ values).

## Usage

To use this driver, import what you need from this crate and an `embedded-hal`
implementation, then instantiate the device.

```rust,no_run
#[cfg(target_os = "linux")]
mod linux {
    use embedded_hal::delay::DelayNs;
    use embedded_sgp30::{Sgp30, DEFAULT_I2C_ADDRESS};
    use linux_embedded_hal as hal;

    pub fn main() -> Result<(), embedded_sgp30::Error<hal::I2CError>> {
        // Create the I2C device from the chosen embedded-hal implementation,
        // in this case linux-embedded-hal
        let mut i2c = match hal::I2cdev::new("/dev/i2c-1") {
            Err(err) => {
                eprintln!("Could not create I2C device: {}", err);
                std::process::exit(1);
            }
            Ok(i2c) => i2c,
        };
        if let Err(err) = i2c.set_slave_address(DEFAULT_I2C_ADDRESS as u16) {
            eprintln!("Could not set I2C slave address: {}", err);
            std::process::exit(1);
        }

        // Create the sensor and configure its repeatability
        let mut sensor = Sgp30::new(i2c, DEFAULT_I2C_ADDRESS, hal::Delay {})?;
        sensor.initialize_air_quality_measure()?;

        // Perform an air quality measurement every second
        let mut delay = hal::Delay {};
        for i in 0..30 {
            delay.delay_ms(1000);
            let measurement = sensor.measure_air_quality()?;

            // Only print the measurement after the startup time of 15s
            if i > 15 {
                println!(
                    "CO₂eq: {} ppm, TVOC: {} ppb",
                    measurement.co2, measurement.tvoc
                );
            }
        }
        Ok(())
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    let _ = linux::main();
    #[cfg(not(target_os = "linux"))]
    println!("This example only works on Linux");
}
```

## Correct usage of baseline get & set

The saving and restore of baseline values should not be done at any random
time.

If starting the sensor without restoring a previous baseline, the sensor will
try to determine a new baseline. For that, the adjustment algorithm has to run
for 12 hours. Therefore, you should not save the baseline values during these
12 hours. Reading out the baseline prior to that should be avoided unless a
valid baseline has first been restored.

After these 12 hours, or if a baseline has been restored at startup, the
baseline should be stored approximately once per hour. If the sensor is off for
some time, the stored baseline values can be stored for a maximum of 7 days.
If the sensor is off for a longer time, the stored baseline should be erased
and the process started again from the beginning, so you should wait once again
12 hours before storing the new baseline.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
