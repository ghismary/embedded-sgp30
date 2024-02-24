use embedded_hal::delay::DelayNs;
use embedded_sgp30::{Sgp30, I2C_ADDRESS};
use linux_embedded_hal as hal;

fn main() -> Result<(), embedded_sgp30::Error<hal::I2CError>> {
    // Create the I2C device from the chosen embedded-hal implementation,
    // in this case linux-embedded-hal
    let mut i2c = match hal::I2cdev::new("/dev/i2c-1") {
        Err(err) => {
            eprintln!("Could not create I2C device: {}", err);
            std::process::exit(1);
        }
        Ok(i2c) => i2c,
    };
    if let Err(err) = i2c.set_slave_address(I2C_ADDRESS as u16) {
        eprintln!("Could not set I2C slave address: {}", err);
        std::process::exit(1);
    }

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
