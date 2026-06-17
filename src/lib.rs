#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs)]
#![no_std]

use crc::{Crc, CRC_8_NRSC_5};
#[allow(unused_imports)]
use micromath::F32Ext;

#[cfg(not(feature = "async"))]
use embedded_hal as hal;
#[cfg(feature = "async")]
use embedded_hal_async as hal;

use hal::i2c::{Operation, SevenBitAddress};

/// The I2C address of the SGP30 chip
pub const DEFAULT_I2C_ADDRESS: SevenBitAddress = 0x58;

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
    I2cE: hal::i2c::Error,
{
    /// I²C bus error
    I2c(I2cE),
    /// The SGP30 chip has not been detected
    ChipNotDetected,
    /// The detected chip is an invalid product, either it has an invalid
    /// product type, or an invalid product version
    InvalidProduct,
    /// The operation that is asked for is not supported by this version of
    /// the sensor.
    FeatureNotSupported,
    /// The computed CRC and the one sent by the device mismatch
    BadCrc,
}

impl<I2cE> From<I2cE> for Error<I2cE>
where
    I2cE: hal::i2c::Error,
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
    product_version: u8,
}

impl<I2C, D> Sgp30<I2C, D>
where
    I2C: hal::i2c::I2c,
    D: hal::delay::DelayNs,
{
    /// Create a new instance of the SGP30 device.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn new(
        i2c: I2C,
        address: SevenBitAddress,
        delay: D,
    ) -> Result<Self, Error<I2C::Error>> {
        let mut device = Self {
            address,
            delay,
            i2c,
            product_version: 0,
        };

        // Check that the chip is present
        if device.get_serial_id().await.is_err() {
            return Err(Error::ChipNotDetected);
        }

        // Check the feature set
        let feature_set_version = device.get_feature_set_version().await?;
        let product_type = (feature_set_version & 0xf000) >> 12;
        let product_version = (feature_set_version & 0x00ff) as u8;
        if product_type != 0 || product_version == 0 {
            return Err(Error::InvalidProduct);
        }
        device.product_version = product_version;

        Ok(device)
    }

    /// Get the baseline values for the air quality signals.
    ///
    /// The goal of this feature is to save the baseline at regular intervals
    /// on an external non-volatile memory and be able to restore these values
    /// after a new power-up or a soft reset of the sensor.
    ///
    /// See [`Sgp30::set_baseline()`] for instructions on how to restore the
    /// values that have been saved here.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn get_baseline(&mut self) -> Result<AirQuality, Error<I2C::Error>> {
        self.get_air_quality(GET_BASELINE_COMMAND, 10).await
    }

    /// Start the air quality measurement
    ///
    /// After this function has been called, the
    /// [`Sgp30::measure_air_quality()`] function has to be called at regular
    /// intervals of 1s to ensure the proper operation of the dynamic baseline
    /// compensation algorithm.
    /// This has to be called after every power-up or after each soft reset
    /// performed with [`Sgp30::reset()`].
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn initialize_air_quality_measure(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.address, INIT_AIR_QUALITY_COMMAND)
            .await?;
        self.delay.delay_ms(10).await;
        Ok(())
    }

    /// Measure the air quality (CO₂eq and TVOC).
    ///
    /// This function has to be called at regular intervals of 1s after the air
    /// quality measure has been initialized with
    /// [`Sgp30::initialize_air_quality_measure()`], to ensure the proper
    /// operation of the dynamic baseline compensation algorithm.
    ///
    /// For the first 15s after the initialization, the sensor is in an
    /// initialization phase and this function will return an air quality
    /// measure with fixed values of 400 ppm CO₂eq and 0 ppb TVOC.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn measure_air_quality(&mut self) -> Result<AirQuality, Error<I2C::Error>> {
        self.get_air_quality(MEASURE_AIR_QUALITY_COMMAND, 12).await
    }

    /// Measure the raw signals (H₂ and ethanol).
    ///
    /// <div class="warning">This is intended for part verification and testing
    /// purposes, therefore you should not need it.</div>
    ///
    /// It returns the raw signals of the sensor.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn measure_raw_signals(&mut self) -> Result<RawSignals, Error<I2C::Error>> {
        self.i2c
            .write(self.address, MEASURE_RAW_SIGNALS_COMMAND)
            .await?;
        self.delay.delay_ms(25).await;
        let mut data = [0u8; 6];
        self.i2c.read(self.address, &mut data).await?;
        let h2: &[u8; 2] = &data[0..2].try_into().unwrap();
        let h2_crc = data[2];
        let ethanol: &[u8; 2] = &data[3..5].try_into().unwrap();
        let ethanol_crc = data[5];
        Self::check_crc(h2, h2_crc)?;
        Self::check_crc(ethanol, ethanol_crc)?;
        Ok(RawSignals {
            h2: Self::get_u16_value(h2),
            ethanol: Self::get_u16_value(ethanol),
        })
    }

    /// Perform a soft reset.
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn reset(&mut self) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, RESET_COMMAND).await?;
        self.delay.delay_us(600).await; // Wait for the sensor to enter idle state
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
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn set_baseline(&mut self, baseline: AirQuality) -> Result<(), Error<I2C::Error>> {
        let mut data = [0u8; 8];
        data[0..2].clone_from_slice(SET_BASELINE_COMMAND);
        let tvoc = Self::get_u8_array_value(baseline.tvoc);
        data[2..4].clone_from_slice(&tvoc);
        data[4] = Self::calc_crc(&tvoc);
        let co2 = Self::get_u8_array_value(baseline.co2);
        data[5..7].clone_from_slice(&co2);
        data[7] = Self::calc_crc(&co2);
        self.i2c.write(self.address, &data).await?;
        self.delay.delay_ms(10).await;
        Ok(())
    }

    /// Feed the on-chip humidity compensation algorithm with the current
    /// humidity to get more accurate air quality measurements.
    ///
    /// The given humidity is the absolute humidity in g/m³, that needs to be
    /// measured with an external sensor such as the SHT3x.
    ///
    /// <div class="warning">This feature may not be available depending on
    /// the version of your sensor.</div>
    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    pub async fn set_humidity(&mut self, humidity: f32) -> Result<(), Error<I2C::Error>> {
        if self.product_version < 0x20 {
            return Err(Error::FeatureNotSupported);
        }
        let mut data = [0u8; 5];
        data[0..2].clone_from_slice(SET_HUMIDITY_COMMAND);
        let humidity = [
            humidity.trunc() as u8,
            (humidity.fract() * 256.0).trunc() as u8,
        ];
        data[2..4].clone_from_slice(&humidity);
        data[4] = Self::calc_crc(&humidity);
        self.i2c.write(self.address, &data).await?;
        self.delay.delay_ms(10).await;
        Ok(())
    }

    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    async fn get_air_quality(
        &mut self,
        command: &[u8],
        wait: u32,
    ) -> Result<AirQuality, Error<I2C::Error>> {
        self.i2c.write(self.address, command).await?;
        self.delay.delay_ms(wait).await;
        let mut data = [0u8; 6];
        self.i2c.read(self.address, &mut data).await?;
        let co2: &[u8; 2] = &data[0..2].try_into().unwrap();
        let co2_crc = data[2];
        let tvoc = &data[3..5].try_into().unwrap();
        let tvoc_crc = data[5];
        Self::check_crc(co2, co2_crc)?;
        Self::check_crc(tvoc, tvoc_crc)?;
        Ok(AirQuality {
            co2: Self::get_u16_value(co2),
            tvoc: Self::get_u16_value(tvoc),
        })
    }

    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    async fn get_feature_set_version(&mut self) -> Result<u16, Error<I2C::Error>> {
        let mut data = [0u8; 3];
        let mut operations = [
            Operation::Write(GET_FEATURE_SET_VERSION_COMMAND),
            Operation::Read(&mut data),
        ];
        self.i2c.transaction(self.address, &mut operations).await?;
        let feature_set_version: &[u8; 2] = &data[0..2].try_into().unwrap();
        let feature_set_version_crc = data[2];
        Self::check_crc(feature_set_version, feature_set_version_crc)?;
        Ok(Self::get_u16_value(feature_set_version))
    }

    #[maybe_async_cfg::maybe(
        sync(not(feature = "async"), keep_self),
        async(feature = "async", keep_self)
    )]
    async fn get_serial_id(&mut self) -> Result<u64, Error<I2C::Error>> {
        self.i2c.write(self.address, GET_SERIAL_ID_COMMAND).await?;
        self.delay.delay_us(500).await;
        let mut data = [0u8; 9];
        self.i2c.read(self.address, &mut data).await?;
        let id1: &[u8; 2] = &data[0..2].try_into().unwrap();
        let id1_crc = data[2];
        let id2: &[u8; 2] = &data[3..5].try_into().unwrap();
        let id2_crc = data[5];
        let id3: &[u8; 2] = &data[6..8].try_into().unwrap();
        let id3_crc = data[8];
        Self::check_crc(id1, id1_crc)?;
        Self::check_crc(id2, id2_crc)?;
        Self::check_crc(id3, id3_crc)?;
        Ok((Self::get_u16_value(id1) as u64) << 32
            | (Self::get_u16_value(id2) as u64) << 16
            | (Self::get_u16_value(id3) as u64))
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
    use embedded_hal::i2c::ErrorKind;
    use embedded_hal_mock::eh1::delay::StdSleep as Delay;
    use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as I2cTransaction};

    use super::*;

    fn create_device() -> Sgp30<I2cMock, Delay> {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x01, 0x02, 0x17, 0x03, 0x04, 0x68, 0x05, 0x06, 0x50].to_vec(),
            ),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                GET_FEATURE_SET_VERSION_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x20, 0x07].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let i2c = I2cMock::new(&expectations);
        let mut device = Sgp30::new(i2c, DEFAULT_I2C_ADDRESS, Delay {}).unwrap();
        device.i2c.done();
        device
    }

    #[test]
    fn chip_not_detected() {
        let expectations =
            [
                I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec())
                    .with_error(ErrorKind::Other),
            ];
        let mut i2c = I2cMock::new(&expectations);
        assert!(matches!(
            Sgp30::new(i2c.by_ref(), DEFAULT_I2C_ADDRESS, Delay {}),
            Err(Error::ChipNotDetected)
        ));
        i2c.done();
    }

    #[test]
    fn invalid_product() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x01, 0x02, 0x17, 0x03, 0x04, 0x68, 0x05, 0x06, 0x50].to_vec(),
            ),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                GET_FEATURE_SET_VERSION_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00, 0x81].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        assert!(matches!(
            Sgp30::new(i2c.by_ref(), DEFAULT_I2C_ADDRESS, Delay {}),
            Err(Error::InvalidProduct)
        ));
        i2c.done();
    }

    #[test]
    fn bad_crc() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x01, 0x02, 0x17, 0x03, 0x04, 0x68, 0x05, 0x06, 0x50].to_vec(),
            ),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                GET_FEATURE_SET_VERSION_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x00, 0x07].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let mut i2c = I2cMock::new(&expectations);
        assert!(matches!(
            Sgp30::new(i2c.by_ref(), DEFAULT_I2C_ADDRESS, Delay {}),
            Err(Error::BadCrc)
        ));
        i2c.done();
    }

    #[test]
    fn get_baseline() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_BASELINE_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x02, 0x76, 0x06, 0x02, 0xdd, 0x10].to_vec(),
            ),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.get_baseline().unwrap();
        device.i2c.done();
    }

    #[test]
    fn initialize_air_quality_measure() {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
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
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, MEASURE_AIR_QUALITY_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x02, 0x76, 0x06, 0x02, 0xdd, 0x10].to_vec(),
            ),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.measure_air_quality().unwrap();
        device.i2c.done();
    }

    #[test]
    fn measure_raw_signals() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, MEASURE_RAW_SIGNALS_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x00, 0x24, 0xc3, 0x01, 0x51, 0x3a].to_vec(),
            ),
        ];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.measure_raw_signals().unwrap();
        device.i2c.done();
    }

    #[test]
    fn reset() {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            RESET_COMMAND.to_vec(),
        )];
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
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            [
                SET_BASELINE_COMMAND[0],
                SET_BASELINE_COMMAND[1],
                0x02,
                0xdd,
                0x10,
                0x02,
                0x76,
                0x06,
            ]
            .to_vec(),
        )];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.set_baseline(air_quality).unwrap();
        device.i2c.done();
    }

    #[test]
    fn set_humidity() {
        let expectations = [I2cTransaction::write(
            DEFAULT_I2C_ADDRESS,
            [
                SET_HUMIDITY_COMMAND[0],
                SET_HUMIDITY_COMMAND[1],
                0x09,
                0x35,
                0x72,
            ]
            .to_vec(),
        )];
        let mut device = create_device();
        device.i2c.update_expectations(&expectations);
        device.set_humidity(9.21).unwrap();
        device.i2c.done();
    }

    #[test]
    fn set_humidity_feature_not_supported() {
        let expectations = [
            I2cTransaction::write(DEFAULT_I2C_ADDRESS, GET_SERIAL_ID_COMMAND.to_vec()),
            I2cTransaction::read(
                DEFAULT_I2C_ADDRESS,
                [0x01, 0x02, 0x17, 0x03, 0x04, 0x68, 0x05, 0x06, 0x50].to_vec(),
            ),
            I2cTransaction::transaction_start(DEFAULT_I2C_ADDRESS),
            I2cTransaction::write(
                DEFAULT_I2C_ADDRESS,
                GET_FEATURE_SET_VERSION_COMMAND.to_vec(),
            ),
            I2cTransaction::read(DEFAULT_I2C_ADDRESS, [0x00, 0x1A, 0x19].to_vec()),
            I2cTransaction::transaction_end(DEFAULT_I2C_ADDRESS),
        ];
        let i2c = I2cMock::new(&expectations);
        let mut device = Sgp30::new(i2c, DEFAULT_I2C_ADDRESS, Delay {}).unwrap();
        assert!(matches!(
            device.set_humidity(9.21),
            Err(Error::FeatureNotSupported)
        ));
        device.i2c.done();
    }
}
