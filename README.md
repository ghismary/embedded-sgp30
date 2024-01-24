This is a platform agnostic Rust driver the SGP30 digital gas sensor using the
[`embedded-hal`] traits.

[`embedded-hal`]: https://github.com/rust-embedded/embedded-hal

## The device

The SGP30 is a gas sensor for indoor air quality applications such as air
purifiers, demand-controlled ventilation or IoT.

It provides detailed information about the air quality, such as TVOC and CO₂eq.
It also gives access to the raw measurement values of ethanol and H₂.

It is addressed via an I²C interface and has a low power consumption.

Here are its measument ranges:

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
- [x] Do a sofware set.
- [x] Perform a raw signals measure (returns the Ethanol and H₂ values).
- [ ] Include a no floating-point variant for systems without fpu.

## Usage

To use this driver, import what you need from this crate and an `embedded-hal`
implentation, then instatiate the device.

```rust,no_run
use embedded_hal::delay::DelayNs;
use embedded_sgp30::{Sgp30, I2C_ADDRESS};
use linux_embedded_hal as hal;

fn main() -> Result<(), embedded_sgp30::Error<hal::I2CError>> {
    // Create the I2C device from the chosen embedded-hal implementation,
    // in this case linux-embedded-hal
    let i2c = match hal::I2cdev::new("/dev/i2c-1") {
        Err(err) => {
            eprintln!("Could not create I2C device: {}", err);
            std::process::exit(1);
        }
        Ok(i2c) => i2c,
    };

    // Create the sensor and configure its repeatability
    let mut sensor = Sgp30::new(i2c, I2C_ADDRESS, hal::Delay {})?;
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
```

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
