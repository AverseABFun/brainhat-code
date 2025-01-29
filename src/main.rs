use byteorder::ByteOrder;
use byteorder::LittleEndian;
use rppal::gpio::Gpio;
#[deny(warnings)]
use rppal::gpio::Level::Low;
use rppal::uart::Uart;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::time;
use std::{thread, time::Duration};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Emotion {
    HighEGood, // High energy good(Elated)
    LowEGood,  // Low energy good(Content)
    HighEBad,  // High energy bad(Furious)
    LowEBad,   // Low energy bad(Disappointed)
    Unset,     // Unset. Default state.
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Item {
    rcn: bool,                  // Reference lead connected. If not, DO NOT trust adc!
    icn: bool,                  // Input lead connected. If not, DO NOT trust adc!
    shdn: bool,                 // Shutdown activated. If so, DO NOT trust rcn, icn, or adc!
    adc: u16,                   // ADC value
    btns: u8,                   // Buttons pressed
    est_max_sampling_rate: u16, // The estimated maximum sampling rate in Hz(as measured via `round(cycles/(time.monotonic()-time0))`, where time0 is the start of the program). Later items will have more accurate sampling rates. This is rounded to an integer.
    timestamp: u128,            // The current unix timestamp in milliseconds.
}

fn writeread_flipper_u16(flipper: &mut Uart, data_address: u8) -> Result<u16, Box<dyn Error>> {
    let mut buf = [0u8, 0];
    flipper.write(&[0b11001000u8, data_address])?;
    flipper.drain()?;
    flipper.read(&mut buf)?;
    Ok(LittleEndian::read_u16(&buf))
}

fn writeread_flipper_u8(flipper: &mut Uart, data_address: u8) -> Result<u8, Box<dyn Error>> {
    let mut buf = [0u8];
    flipper.write(&[0b11001000u8, data_address])?;
    flipper.drain()?;
    flipper.read(&mut buf)?;
    Ok(buf[0])
}

fn read_adc(flipper: &mut Uart) -> Result<u16, Box<dyn Error>> {
    writeread_flipper_u16(flipper, 0u8)
}

fn read_keys(flipper: &mut Uart) -> Result<u8, Box<dyn Error>> {
    writeread_flipper_u8(flipper, 1u8)
}

fn read_status(flipper: &mut Uart) -> Result<bool, Box<dyn Error>> {
    let stat = writeread_flipper_u16(flipper, 2u8)?;
    if stat == 0x4F4B {
        // 4F4B="OK"
        return Ok(true);
    }
    Ok(false)
}

fn read_est_max_sample_rate(flipper: &mut Uart) -> Result<u16, Box<dyn Error>> {
    writeread_flipper_u16(flipper, 3)
}

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::args().collect::<Vec<String>>().len() >= 2 {
        if std::env::args().collect::<Vec<String>>()[1] == "clear" {
            let _ = fs::remove_file("/mnt/share/data/data.bh");
        }
    }
    println!("Creating data");
    let rcn = Gpio::new()?.get(16)?.into_input_pullup();
    let icn = Gpio::new()?.get(20)?.into_input_pullup();
    let mut shdn = Gpio::new()?.get(21)?.into_output();
    let mut reset = Gpio::new()?.get(18)?.into_output();
    shdn.set_high();
    reset.set_high();
    thread::sleep(Duration::from_millis(1500));
    reset.set_low();

    let mut flipper = Uart::new(230400, rppal::uart::Parity::None, 8, 1)?;
    flipper.set_write_mode(true)?;
    flipper.set_read_mode(255, Duration::from_millis(1000))?; // Will block until buffer is full or timeout is reached.

    while !read_status(&mut flipper).unwrap_or(false) {}

    let mut cache: Vec<Item> = vec![];

    loop {
        let start = time::SystemTime::now();
        let timestamp = start
            .duration_since(time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();
        let keys = read_keys(&mut flipper)?;
        let item = Item {
            rcn: rcn.read() == Low,
            icn: icn.read() == Low,
            shdn: shdn.is_set_low(),
            adc: read_adc(&mut flipper)?,
            btns: keys,
            est_max_sampling_rate: read_est_max_sample_rate(&mut flipper)?,
            timestamp,
        };
        cache.push(item);
        if cache.len() >= 400 {
            println!("{}", read_est_max_sample_rate(&mut flipper)?);
            fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/mnt/share/data/data.bh")?
                .write(serde_json::to_string(&cache)?.as_bytes())?;
            cache.clear();
            println!("{}", read_est_max_sample_rate(&mut flipper)?);
        }
        thread::sleep(Duration::from_secs_f64(1.0 / 400.0));
    }
}
