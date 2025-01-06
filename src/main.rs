use byteorder::BigEndian;
use byteorder::ByteOrder;
use rppal::gpio::Gpio;
use rppal::uart::Uart;
use std::error::Error;
use std::{thread, time::Duration};
use anyhow::anyhow;

fn writeread_metro_u16(metro: &mut Uart, data_address: u8) -> Result<u16, Box<dyn Error>> {
    let mut buf = [0u8, 0];
    metro.write(&[0b11001000u8, data_address])?;
    metro.drain()?;
    metro.read(&mut buf)?;
    Ok(BigEndian::read_u16(&buf))
}

fn writeread_metro_u8(metro: &mut Uart, data_address: u8) -> Result<u8, Box<dyn Error>> {
    let mut buf = [0u8];
    metro.write(&[0b11001000u8, data_address])?;
    metro.drain()?;
    metro.read(&mut buf)?;
    Ok(buf[0])
}

fn write_metro(metro: &mut Uart, data_address: u8, data: Vec<u8>) -> Result<bool, Box<dyn Error>> {
    let mut buf = [0u8];
    metro.write(&[0b01001000u8, data_address, data.len().to_le_bytes()[0]])?;
    metro.write(data.as_slice())?;
    metro.drain()?;
    metro.read(&mut buf)?;
    Ok(buf[0] == 0)
}

fn read_adc(metro: &mut Uart) -> Result<u16, Box<dyn Error>> {
    writeread_metro_u16(metro, 0u8)
}

fn read_keys(metro: &mut Uart) -> Result<u8, Box<dyn Error>> {
    writeread_metro_u8(metro, 1u8)
}

fn write_pixel(metro: &mut Uart, pixel: u8, color: (u8,u8,u8)) -> Result<bool, Box<dyn Error>> {
    write_metro(metro, pixel, vec![color.0, color.1, color.2])
}

fn write_pixel_brightness(metro: &mut Uart, pixel: u8, brightness: u8) -> Result<bool, Box<dyn Error>> {
    write_metro(metro, pixel+5, vec![brightness])
}

fn read_status(metro: &mut Uart) -> Result<bool, Box<dyn Error>> {
    let stat = writeread_metro_u16(metro, 2)?;
    if stat == 0x4F4B { // 4F4B="OK"
        return Ok(true);
    }
    Ok(false)
}

fn main() -> Result<(), Box<dyn Error>> {
    let rcn = Gpio::new()?.get(16)?.into_input_pullup();
    let icn = Gpio::new()?.get(20)?.into_input_pullup();
    let mut shdn = Gpio::new()?.get(21)?.into_output();
    let mut reset = Gpio::new()?.get(18)?.into_output();
    shdn.set_high();
    reset.set_high();
    thread::sleep(Duration::from_millis(1500));
    reset.set_low();

    let mut metro = Uart::new(9600, rppal::uart::Parity::None, 8, 1)?;
    metro.set_write_mode(true)?;
    metro.set_read_mode(0, Duration::from_millis(3000))?;

    while !read_status(&mut metro)? {}

    if !write_pixel(&mut metro, 0, (255, 0, 0))? {
        return Err(anyhow!("error writing to neopixel 0").into());
    }

    if !write_pixel_brightness(&mut metro, 3, 127)? {
        return Err(anyhow!("error writing to neopixel 3").into());
    }

    loop {
        println!("rcn: {}", rcn.read());
        println!("icn: {}", icn.read());
        println!("adc: {}", read_adc(&mut metro)?);
        println!("buttons pressed: {:08b}", read_keys(&mut metro)?);
        thread::sleep(Duration::from_secs(1))
    }
}
