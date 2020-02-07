//#[macro_use]
//extern crate rouille;

use std::{thread, time};
use std::error::Error;

use rppal::i2c::I2c;

const ADDRESS_BME_280: u16 = 0x76;

const REGISTER_CHIP_ID: usize = 0xD0;

const REGISTER_DATA: usize = 0xF7;
const REGISTER_CONTROL: usize = 0xF4;

const REGISTER_CONTROL_HUMIDITY: usize = 0xF2;

const MODE: usize = 2;
const OVERSAMPLE_TEMPERATURE: usize = 2;
const OVERSAMPLE_PRESSURE: usize = 2;
const OVERSAMPLE_HUMIDITY: usize = 2;

fn read_id(i2c: &I2c) -> Result<String, Box<dyn Error>> {
    let mut reg = [0u8; 2];
    i2c.block_read(REGISTER_CHIP_ID as u8, &mut reg)?;
    return Ok(format!("ChipID: {}, ChipVersion: {}", reg[0], reg[1]));
}

fn read_data(i2c: &I2c) -> Result<Vec<f64>, Box<dyn Error>> {
    i2c.smbus_write_byte(REGISTER_CONTROL_HUMIDITY as u8, OVERSAMPLE_PRESSURE as u8)?;

    let control = OVERSAMPLE_TEMPERATURE << 5 | OVERSAMPLE_PRESSURE << 2 | MODE;
    i2c.smbus_write_byte(REGISTER_CONTROL as u8, control as u8)?;
    // Read compensation parameters
    // temperature
    let dig_t1 = i2c.smbus_read_word(0x88)?;
    let dig_t2 = i2c.smbus_read_word(0x8A)? as i16;
    let dig_t3 = i2c.smbus_read_word(0x8C)? as i16;

    // pressure
    let dig_p1 = i2c.smbus_read_word(0x8E)?;
    let dig_p2 = i2c.smbus_read_word(0x90)? as i16;
    let dig_p3 = i2c.smbus_read_word(0x92)? as i16;
    let dig_p4 = i2c.smbus_read_word(0x94)? as i16;
    let dig_p5 = i2c.smbus_read_word(0x96)? as i16;
    let dig_p6 = i2c.smbus_read_word(0x98)? as i16;
    let dig_p7 = i2c.smbus_read_word(0x9A)? as i16;
    let dig_p8 = i2c.smbus_read_word(0x9C)? as i16;
    let dig_p9 = i2c.smbus_read_word(0x9E)? as i16;

    // humidity
    let dig_h1 = i2c.smbus_read_byte(0xA1)?;
    let dig_h2 = i2c.smbus_read_word(0xE1)? as i16;
    let dig_h3 = i2c.smbus_read_byte(0xE3)?;

    // H4 and H5 are sharing a ... bit? What?
    let mut dig_h_4_to_5 = [0u8; 3];
    i2c.block_read(0xE4, &mut dig_h_4_to_5)?;
    let mut dig_h4 = u32::from(dig_h_4_to_5[0]) << 24 >> 20;
    dig_h4 = dig_h4 | (u32::from(dig_h_4_to_5[1]) & 0x0F);
    let mut dig_h5 = u32::from(dig_h_4_to_5[2]) << 24 >> 20;
    dig_h5 = dig_h5 | (u32::from(dig_h_4_to_5[1]) >> 4 & 0x0F);

    let dig_h6 = i2c.smbus_read_byte(0xE7)?;

    let wait_time_ms = (1.25 +
        (2.3 * OVERSAMPLE_TEMPERATURE as f64) +
        ((2.3 * OVERSAMPLE_PRESSURE as f64) + 0.575) +
        ((2.3 * OVERSAMPLE_HUMIDITY as f64) + 0.575)) as u64;

    thread::sleep(time::Duration::from_millis(wait_time_ms));

    let mut data = [0u8; 8];
    i2c.block_read(REGISTER_DATA as u8, &mut data)?;
    let pressure_raw = u32::from(data[0]) << 12 | u32::from(data[1]) << 4 | u32::from(data[2]) >> 12;
    let temperature_raw = u32::from(data[3]) << 12 | u32::from(data[4]) << 4 | u32::from(data[5]) >> 12;
    let humidity_raw = u32::from(data[6]) << 8 | u32::from(data[7]);

    let t_fine: f64 = temperature_fine(dig_t1, dig_t2, dig_t3, temperature_raw);
    let temperature = refine_temperature(t_fine);
    let pressure = refine_pressure(t_fine, dig_p1, dig_p2, dig_p3, dig_p4, dig_p5, dig_p6,
                                   dig_p7, dig_p8, dig_p9, pressure_raw);
    let humidity = refine_humidity(t_fine, dig_h1, dig_h2, dig_h3, dig_h4 as i16, dig_h5 as i16, dig_h6, humidity_raw);
    return Ok(vec![temperature / 100.0, pressure / 100.0, humidity]);
}

fn refine_temperature(t_fine: f64) -> f64 {
    let temperature = ((t_fine as u32 * 5) + 128) >> 8;
    temperature as f64
}

fn temperature_fine(dig_t1: u16, dig_t2: i16, dig_t3: i16, temperature_raw: u32) -> f64 {
    let var_1 = (((temperature_raw >> 3) - (dig_t1 << 1) as u32) * (dig_t2 as u32)) >> 11;
    let var_2 = (((((temperature_raw >> 4) - (dig_t1) as u32) * ((temperature_raw >> 4) - (dig_t1) as u32)) >> 12) * (dig_t3 as u32)) >> 14;
    return f64::from(var_1 + var_2);
}

// Refine pressure and adjust for temperature
fn refine_pressure(t_fine: f64, dig_p1: u16, dig_p2: i16, dig_p3: i16, dig_p4: i16, dig_p5: i16,
                   dig_p6: i16, dig_p7: i16, dig_p8: i16, dig_p9: i16, pressure_raw: u32) -> f64 {
    let mut var_1 = t_fine / 2.0 - 64000.0;
    let mut var_2 = var_1 * var_1 * f64::from(dig_p6) / 32768.0;
    var_2 = var_2 + var_1 * f64::from(dig_p5) * 2.0;
    var_2 = var_2 / 4.0 + f64::from(dig_p4) * 65536.0;
    var_1 = (f64::from(dig_p3) * var_1 * var_1 / 524288.0 + f64::from(dig_p2) * var_1) / 524288.0;
    var_1 = (1.0 + var_1 / 32768.0) * f64::from(dig_p1);

    return if var_1 == 0.0 {
        0.0
    } else {
        let mut pressure = 1048576.0 - f64::from(pressure_raw);
        pressure = ((pressure - var_2 / 4096.0) * 6250.0) / var_1;
        var_1 = f64::from(dig_p9) * pressure * pressure / 2147483648.0;
        var_2 = pressure * f64::from(dig_p8) / 32768.0;
        pressure + (var_1 + var_2 + f64::from(dig_p7)) / 16.0
    };
}

fn refine_humidity(t_fine: f64, dig_h1: u8, dig_h2: i16, dig_h3: u8, dig_h4: i16, dig_h5: i16,
                   dig_h6: u8, humidity_raw: u32) -> f64 {
    let mut humidity = t_fine - 76800.0;
    // Wtf
    humidity = (f64::from(humidity_raw) -
        (
            f64::from(dig_h4) * 64.0 + f64::from(dig_h5) / 16384.0 * humidity)
    ) *
        (f64::from(dig_h2) / 65536.0 *
            (1.0 +
                (f64::from(dig_h6) / 67108864.0 * humidity *
                    (1.0 + f64::from(dig_h3) / 67108864.0 * humidity)
                )
            )
        );
    humidity = humidity * (1.0 - f64::from(dig_h1) * humidity / 524288.0);
    humidity.min(100.0).max(0.0)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut i2c = I2c::with_bus(1)?;
    i2c.set_slave_address(ADDRESS_BME_280)?;
    let info = read_id(&i2c)?;
    println!("Chip information: {}", info);

    let vals = read_data(&i2c)?;
    println!("Temperature: {} C", vals[0]);
    println!("Pressure: {:.4} hPa", vals[1]);
    println!("Humidity: {:.2} %", vals[2]);
    return Ok(());
//    println!("Now listening on 0.0.0.0:8000");

//    // The `start_server` starts listening forever on the given address.
//    rouille::start_server("0.0.0.0:8000", move |request| {
//        // The closure passed to `start_server` will be called once for each client request. It
//        // will be called multiple times concurrently when there are multiple clients.
//
//        // Here starts the real handler for the request.
//        //
//        // The `router!` macro is very similar to a `match` expression in core Rust. The macro
//        // takes the request as parameter and will jump to the first block that matches the
//        // request.
//        //
//        // Each of the possible blocks builds a `Response` object. Just like most things in Rust,
//        // the `router!` macro is an expression whose value is the `Response` built by the block
//        // that was called. Since `router!` is the last piece of code of this closure, the
//        // `Response` is then passed back to the `start_server` function and sent to the client.
//        router!(request,
//            (GET) (/) => {
//                rouille::Response::text("hello world")
//            },
//
//            (GET) (/metrics) => {
//                rouille::Response::text("There will be metrics")
//            },
//            // The code block is called if none of the other blocks matches the request.
//            // We return an empty response with a 404 status code.
//            _ => rouille::Response::empty_404()
//        )
//    });
}


