# Sensador

A small rust library to read the BME/BMP280 sensor and report them as prometheus metrics. Using rouille
as a web library.

## Requirements

Make sure you have I2C enabled and the necessary tools installed! 

Make sure your chip is connected properly. Test it with `i2cdetect -y 1`, which should show a `76` in the matrix.

## Usage

Start the program:
`./sensador`

It should report the current values and open a local webserver at port 8000

## Compilation

Use the provided `docker-compose.yml` to compile the file for a raspberry zero:

```shell script
docker-compose run --rm rpi0 bash
cargo build 
# OR for a release build
cargo build --release  
```

The program should be in `target/arm-unknown-linux-musleabihf/{debug OR release}/sensador`. 
