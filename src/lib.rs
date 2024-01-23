#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs)]
#![no_std]

use crc::{Crc, CRC_8_NRSC_5};
use embedded_hal::i2c::{Operation, SevenBitAddress};
#[allow(unused_imports)]
use micromath::F32Ext;

/// The I2C address of the SGP30 chip
pub const I2C_ADDRESS: SevenBitAddress = 0x58;

const GET_BASELINE_COMMAND: &[u8] = &[0x20, 0x15];
const GET_FEATURE_SET_VERSION_COMMAND: &[u8] = &[0x20, 0x2f];
const GET_SERIAL_ID_COMMAND: &[u8] = &[0x36, 0x82];
const INIT_AIR_QUALITY_COMMAND: &[u8] = &[0x20, 0x03];
const MEASURE_AIR_QUALITY_COMMAND: &[u8] = &[0x20, 0x08];
const MEASURE_RAW_SIGNALS_COMMAND: &[u8] = &[0x20, 0x50];
const RESET_COMMAND: &[u8] = &[0x00, 0x06];
const SET_BASELINE_COMMAND: &[u8] = &[0x20, 0x1e];
const SET_HUMIDITY_COMMAND: &[u8] = &[0x20, 0x61];

/// All possible errors generated when using the Sgp30 struct
#[derive(Debug)]
pub enum Error<I2cE>
where
    I2cE: embedded_hal::i2c::Error,
{
    /// I²C bus error
    I2c(I2cE),
    /// The SGP30 chip has not been detected
    ChipNotDetected,
    /// The detected chip is an invalid product, either it has an invalid
    /// product type, or an invalid product version
    InvalidProduct,
    /// The computed CRC and the one sent by the device mismatch
    BadCrc,
}

impl<I2cE> From<I2cE> for Error<I2cE>
where
    I2cE: embedded_hal::i2c::Error,
{
    fn from(value: I2cE) -> Self {
        Error::I2c(value)
    }
}

/// The result of an air quality measurement.
#[derive(Clone, Copy, Debug, Default)]
pub struct AirQuality {
    /// The value of the CO₂ equivalent signal (CO₂eq) in ppm (parts per
    /// million).
    pub co2: u16,
    /// The value of the TVOC signal in ppb (parts per billion).
    pub tvoc: u16,
}

/// The result of a raw signals measurement.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawSignals {
    /// The value of the ethanol signal in ppm (parts per million).
    pub ethanol: u16,
    /// The value of the H₂ signal in ppm (parts per million).
    pub h2: u16,
}

/// SGP30 device driver
#[derive(Debug)]
pub struct Sgp30<I2C, D> {
    address: SevenBitAddress,
    delay: D,
    i2c: I2C,
}

impl<I2C, D> Sgp30<I2C, D>
where
    I2C: embedded_hal::i2c::I2c,
    D: embedded_hal::delay::DelayNs,
{
    /// Get the baseline values for the air quality signals.
    ///
    /// The goal of this feature is to save the baseline at regular intervals
    /// on an external non-volatile memory and be able to restore these values
    /// after a new power-up or a soft reset of the sensor.
    ///
    /// See [`Sgp30::set_baseline()`] for instructions on how to restore the
    /// values that have been saved here.
    pub fn get_baseline(&mut self) -> Result<AirQuality, Error<I2C::Error>> {
        self.get_air_quality(GET_BASELINE_COMMAND, 10)
    }

    /// Start the air quality measurement
    ///
    /// After this function has been called, the
    /// [`Sgp30::measure_air_quality()`] function has to be called at regular
    /// intervals of 1s to ensure the proper operation of the dynamic baseline
    /// compensation algorithm.
    /// This has to be called after every power-up or after each soft reset
    /// performed with [`Sgp30::reset()`].
    pub fn initialize_air_quality_measure(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, INIT_AIR_QUALITY_COMMAND)?;
        self.delay.delay_ms(10);
        Ok(())
    }

    /// Measure the air quality (CO₂eq and TVOC).
    ///
    /// This function has to be called at regular invervals of 1s after the air
    /// quality measure has been initialized with
    /// [`Sgp30::initialize_air_quality_measure()`], to ensure the proper
    /// operation of the dynamic baseline compensation algorithm.
    ///
    /// For the first 15s after the initialization, the sensor is in an
    /// initialization phase and this function will return an air quality
    /// measure with fixed values of 400 ppm CO₂eq and 0 ppb TVOC.
    pub fn measure_air_quality(&mut self) -> Result<AirQuality, Error<I2C::Error>> {
        self.get_air_quality(MEASURE_AIR_QUALITY_COMMAND, 12)
    }

    /// Measure the raw signals (H₂ and ethanol).
    ///
    /// <div class="warning">This is intended for part verification and testing
    /// purposes, therefore you should not need it.</div>
    ///
    /// Itreturns the raw signals of the sensor.
    pub fn measure_raw_signals(&mut self) -> Result<RawSignals, Error<I2C::Error>> {
        self.i2c.write(self.address, MEASURE_RAW_SIGNALS_COMMAND)?;
        self.delay.delay_ms(25);
        let mut h2 = [0u8; 2];
        let mut h2_crc = [0u8; 1];
        let mut ethanol = [0u8; 2];
        let mut ethanol_crc = [0u8; 1];
        let mut operations = [
            Operation::Read(&mut h2),
            Operation::Read(&mut h2_crc),
            Operation::Read(&mut ethanol),
            Operation::Read(&mut ethanol_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&h2, h2_crc[0])?;
        Self::check_crc(&ethanol, ethanol_crc[0])?;
        Ok(RawSignals {
            h2: Self::get_u16_value(&h2),
            ethanol: Self::get_u16_value(&ethanol),
        })
    }

    /// Create a new instance of the SGP30 device.
    pub fn new(i2c: I2C, address: SevenBitAddress, delay: D) -> Result<Self, Error<I2C::Error>> {
        let mut device = Self {
            address,
            delay,
            i2c,
        };

        // Check that the chip is present
        if device.get_serial_id().is_err() {
            return Err(Error::ChipNotDetected);
        }

        // Check the feature set
        let feature_set_version = device.get_feature_set_version()?;
        let product_type = (feature_set_version & 0xf000) >> 12;
        let product_version = feature_set_version & 0x00ff;
        if product_type != 0 || product_version == 0 {
            return Err(Error::InvalidProduct);
        }

        Ok(device)
    }

    /// Perform a soft reset.
    pub fn reset(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, RESET_COMMAND)?;
        self.delay.delay_us(600); // Wait for the sensor to enter idle state
        Ok(())
    }

    /// Set the baseline values for the air quality signals.
    ///
    /// The goal of this feature is to feed the baseline correction algorithm
    /// with values that have been stored on an external memory using the
    /// [`Sgp30::get_baseline()`] function.
    ///
    /// This needs to be called just after calling
    /// [`Sgp30::initialize_air_quality_measure()`], and prior to any call to
    /// [`Sgp30::measure_air_quality()`].
    pub fn set_baseline(&mut self, baseline: AirQuality) -> Result<(), Error<I2C::Error>> {
        let co2 = Self::get_u8_array_value(baseline.co2);
        let co2_crc = [Self::calc_crc(&co2)];
        let tvoc = Self::get_u8_array_value(baseline.tvoc);
        let tvoc_crc = [Self::calc_crc(&tvoc)];
        let mut operations = [
            Operation::Write(SET_BASELINE_COMMAND),
            Operation::Write(&co2),
            Operation::Write(&co2_crc),
            Operation::Write(&tvoc),
            Operation::Write(&tvoc_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        self.delay.delay_ms(10);
        Ok(())
    }

    /// Feed the on-chip humidity compensation algorithm with the current
    /// humidity to get more accurate air quality measurements.
    ///
    /// The given humidity is the absolute humidity in g/m³, that needs to be
    /// measured with an external sensor such as the SHT3x.
    pub fn set_humidity(&mut self, humidity: f32) -> Result<(), Error<I2C::Error>> {
        let humidity = [
            humidity.trunc() as u8,
            (humidity.fract() * 256.0).trunc() as u8,
        ];
        let humidity_crc = [Self::calc_crc(&humidity)];
        let mut operations = [
            Operation::Write(SET_HUMIDITY_COMMAND),
            Operation::Write(&humidity),
            Operation::Write(&humidity_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        self.delay.delay_ms(10);
        Ok(())
    }

    fn get_air_quality(
        &mut self,
        command: &[u8],
        wait: u32,
    ) -> Result<AirQuality, Error<I2C::Error>> {
        self.i2c.write(self.address, command)?;
        self.delay.delay_ms(wait);
        let mut co2 = [0u8; 2];
        let mut co2_crc = [0u8; 1];
        let mut tvoc = [0u8; 2];
        let mut tvoc_crc = [0u8; 1];
        let mut operations = [
            Operation::Read(&mut co2),
            Operation::Read(&mut co2_crc),
            Operation::Read(&mut tvoc),
            Operation::Read(&mut tvoc_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&co2, co2_crc[0])?;
        Self::check_crc(&tvoc, tvoc_crc[0])?;
        Ok(AirQuality {
            co2: Self::get_u16_value(&co2),
            tvoc: Self::get_u16_value(&tvoc),
        })
    }

    fn get_feature_set_version(&mut self) -> Result<u16, Error<I2C::Error>> {
        let mut feature_set_version = [0u8; 2];
        let mut feature_set_version_crc = [0u8; 1];
        let mut operations = [
            Operation::Write(GET_FEATURE_SET_VERSION_COMMAND),
            Operation::Read(&mut feature_set_version),
            Operation::Read(&mut feature_set_version_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&feature_set_version, feature_set_version_crc[0])?;
        Ok(Self::get_u16_value(&feature_set_version))
    }

    fn get_serial_id(&mut self) -> Result<u64, Error<I2C::Error>> {
        let mut id1 = [0u8; 2];
        let mut id2 = [0u8; 2];
        let mut id3 = [0u8; 2];
        let mut id1_crc = [0u8; 1];
        let mut id2_crc = [0u8; 1];
        let mut id3_crc = [0u8; 1];
        self.i2c.write(self.address, GET_SERIAL_ID_COMMAND)?;
        self.delay.delay_us(500);
        let mut operations = [
            Operation::Read(&mut id1),
            Operation::Read(&mut id1_crc),
            Operation::Read(&mut id2),
            Operation::Read(&mut id2_crc),
            Operation::Read(&mut id3),
            Operation::Read(&mut id3_crc),
        ];
        self.i2c.transaction(self.address, &mut operations)?;
        Self::check_crc(&id1, id1_crc[0])?;
        Self::check_crc(&id2, id2_crc[0])?;
        Self::check_crc(&id3, id3_crc[0])?;
        Ok((Self::get_u16_value(&id1) as u64) << 32
            | (Self::get_u16_value(&id2) as u64) << 16
            | (Self::get_u16_value(&id3) as u64))
    }

    fn calc_crc(data: &[u8; 2]) -> u8 {
        let crc = Crc::<u8>::new(&CRC_8_NRSC_5);
        let mut digest = crc.digest();
        digest.update(data);
        digest.finalize()
    }

    fn check_crc(data: &[u8; 2], expected_crc: u8) -> Result<(), Error<I2C::Error>> {
        if Self::calc_crc(data) != expected_crc {
            Err(Error::BadCrc)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn get_u8_array_value(data: u16) -> [u8; 2] {
        [(data >> 8) as u8, (data & 0xff) as u8]
    }

    #[inline]
    fn get_u16_value(data: &[u8; 2]) -> u16 {
        (data[0] as u16) << 8 | (data[1] as u16)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use embedded_hal_mock::eh1::delay::StdSleep as Delay;
    use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as I2cTransaction};

    fn create_device() -> Sgp30<I2cMock, Delay> {
        let expectations = [
            I2cTransaction::write(I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec()),
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::read(I2C_ADDRESS, [0x01, 0x02].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x17].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x03, 0x04].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x68].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x05, 0x06].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x50].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::write(I2C_ADDRESS, GET_FEATURE_SET_VERSION_COMMAND.to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x00, 0x20].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x07].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let i2c = I2cMock::new(&expectations);
        let mut device = Sgp30::new(i2c, I2C_ADDRESS, Delay {}).unwrap();
        device.i2c.done();
        device
    }

    #[test]
    fn get_baseline() {
        let expectations = [
            I2cTransaction::write(I2C_ADDRESS, GET_BASELINE_COMMAND.to_vec()),
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::read(I2C_ADDRESS, [0x02, 0x76].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x06].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x02, 0xdd].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x10].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.get_baseline().unwrap();
        device.i2c.done();
    }

    #[test]
    fn initialize_air_quality_measure() {
        let expectations = [I2cTransaction::write(
            I2C_ADDRESS,
            INIT_AIR_QUALITY_COMMAND.to_vec(),
        )];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.initialize_air_quality_measure().unwrap();
        device.i2c.done();
    }

    #[test]
    fn measure_air_quality() {
        let expectations = [
            I2cTransaction::write(I2C_ADDRESS, MEASURE_AIR_QUALITY_COMMAND.to_vec()),
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::read(I2C_ADDRESS, [0x02, 0x76].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x06].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x02, 0xdd].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x10].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.measure_air_quality().unwrap();
        device.i2c.done();
    }

    #[test]
    fn measure_raw_signals() {
        let expectations = [
            I2cTransaction::write(I2C_ADDRESS, MEASURE_RAW_SIGNALS_COMMAND.to_vec()),
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::read(I2C_ADDRESS, [0x00, 0x24].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0xc3].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x01, 0x51].to_vec()),
            I2cTransaction::read(I2C_ADDRESS, [0x3a].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.measure_raw_signals().unwrap();
        device.i2c.done();
    }

    #[test]
    fn reset() {
        let expectations = [I2cTransaction::write(I2C_ADDRESS, RESET_COMMAND.to_vec())];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.reset().unwrap();
        device.i2c.done();
    }

    #[test]
    fn set_baseline() {
        let air_quality = AirQuality {
            co2: 630,
            tvoc: 733,
        };
        let expectations = [
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::write(I2C_ADDRESS, SET_BASELINE_COMMAND.to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x02, 0x76].to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x06].to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x02, 0xdd].to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x10].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.set_baseline(air_quality).unwrap();
        device.i2c.done();
    }

    #[test]
    fn set_humidity() {
        let expectations = [
            I2cTransaction::transaction_start(I2C_ADDRESS),
            I2cTransaction::write(I2C_ADDRESS, SET_HUMIDITY_COMMAND.to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x09, 0x35].to_vec()),
            I2cTransaction::write(I2C_ADDRESS, [0x72].to_vec()),
            I2cTransaction::transaction_end(I2C_ADDRESS),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.set_humidity(9.21).unwrap();
        device.i2c.done();
    }
}
