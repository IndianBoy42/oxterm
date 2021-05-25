#![feature(array_chunks)]
#![feature(with_options)]

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::{stdout, BufWriter, Write};
use std::process::exit;
use std::str;
use std::str::FromStr;
use std::time::Duration;
use std::time::Instant;

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

const HELP: &str = "\
Simple Serial Terminal TUI in the shell 
USAGE:
	app [OPTIONS]

    String values for options are always case insensitive
FLAGS:
	-h, --help            Prints help information
OPTIONS:
	-p --port         STRING     Serial port (/dev/tty* or COMx)
        Not needed if there is only one port
        If not given and there are multiple ports we will just enumerate the ports
	-b --baud-rate    NUMBER     Baud rate to open with [ default: 115200 ]
	-d --data-bits    NUMBER     Data bits (5,6,7,8) [ default: 8 ]
	-F --flow-control STRING     Flow control for the port (None, SW, HW) [default: None]
	-P --parity       STRING     Which parity to use (None, odd, even) [default: None]
	-s --stop-bits    NUMBER     Number of stop bits (1, 2) [default: 1]
	-T --timeout      NUMBER     Timeout (milliseconds) on receiving data [default: 0]
    -m --mode         STRING     stdout, iterm, lines, file  [default: stdout]
    -o --output-file  PATH       File to write to (only relevant with `-m file`) [default: output.txt]
    -a --append       PATH       In file mode we will append the new data [default: false]
    -C --capacity     NUMBER     Buffer capacity for stdout/file writing [default: 64] 
        This is set quite low to be able to see live updates (and because UART is slow usually)
        You could increase this a lot if you are writing to a file and/or using faster data
    -c --convert      STRING     Perform some simple data conversion (all outputs human readable ascii)
        (NO OPT) just view/save the data, this essentially means ASCII
        HEX  convert every byte to hex representation
        BIN  convert every byte to binary representation
        INT  convert every 4 bytes from 32 bit integers 
        SHR  convert every 2 bytes from 16 bit integers 
        U*   unsigned variants of the above 2
        FLT  convert every 4 bytes from 32 bit floating points
    -f --format       PATH       Path to file with parser format (unimplemented)
        Parse binary data into human-readable format for more efficient bandwidth usage
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConvertFrom {
    NON,
    HEX,
    BIN,
    INT,
    UINT,
    USHR,
    SHR,
    FLT,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InvalidConvertFrom;

impl std::fmt::Display for InvalidConvertFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConvertFrom got invalid value")
    }
}
impl FromStr for ConvertFrom {
    type Err = InvalidConvertFrom;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ConvertFrom::*;
        Ok(match s {
            "NON" => NON,
            "HEX" => HEX,
            "BIN" => BIN,
            "INT" => INT,
            "SHR" => SHR,
            "UINT" => UINT,
            "USHR" => USHR,
            "FLT" => FLT,
            _ => return Err(InvalidConvertFrom),
        })
    }
}

#[derive(Debug)]
struct Args {
    port: String,
    baud_rate: u32,
    data_bits: DataBits,
    flow_control: FlowControl,
    parity: Parity,
    stop_bits: StopBits,
    timeout: Duration,
    mode: String,
    capacity: usize,
    output_file: OsString,
    append: bool,
    convertfrom: ConvertFrom,
}
impl Default for Args {
    fn default() -> Self {
        Args {
            port: String::new(),
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            flow_control: FlowControl::None,
            parity: Parity::None,
            stop_bits: StopBits::One,
            timeout: Duration::from_millis(0),
            mode: String::from("stdout"),
            capacity: 64,
            output_file: "output.txt".into(),
            append: false,
            convertfrom: ConvertFrom::NON,
        }
    }
}
fn main() {
    println!("Hello, world!");
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let dargs = Args::default();

    let args = Args {
        port: pargs
            .opt_value_from_str(["-p", "--port"])
            .unwrap()
            .unwrap_or_else(|| {
                let ports = serialport::available_ports().expect("No ports found!");
                if ports.len() == 1 {
                    let mut ports = ports;
                    ports.remove(0).port_name
                } else {
                    // TODO: prettify the output of this section
                    println!("Found ports:");
                    for p in ports {
                        println!("{:?}", p);
                    }
                    exit(0)
                }
            }),

        baud_rate: pargs
            .opt_value_from_str(["-b", "--baud-rate"])
            .unwrap()
            .unwrap_or(dargs.baud_rate),

        data_bits: pargs
            .opt_value_from_fn(["-d", "--data-bits"], |x| {
                Ok(match x {
                    "5" => DataBits::Five,
                    "6" => DataBits::Six,
                    "7" => DataBits::Seven,
                    "8" => DataBits::Eight,
                    _ => return Err("Data bits option passed an invalid value"),
                })
            })
            .unwrap()
            .unwrap_or(dargs.data_bits),

        flow_control: pargs
            .opt_value_from_fn(["-F", "--flow-control"], |n| {
                Ok(match n.to_ascii_lowercase().as_str() {
                    "hardware" | "hw" => FlowControl::Hardware,
                    "software" | "sw" => FlowControl::Software,
                    "none" => FlowControl::None,
                    _ => return Err("Flow Control option passed an invalid value"),
                })
            })
            .unwrap()
            .unwrap_or(dargs.flow_control),

        parity: pargs
            .opt_value_from_fn(["-P", "--parity"], |n| {
                Ok(match n.to_ascii_lowercase().as_str() {
                    "none" => Parity::None,
                    "odd" => Parity::Odd,
                    "even" => Parity::Even,
                    _ => return Err("Parity option passed an invalid value"),
                })
            })
            .unwrap()
            .unwrap_or(dargs.parity),

        stop_bits: pargs
            .opt_value_from_fn(["-s", "--stop-bits"], |n| {
                Ok(match n.to_ascii_lowercase().as_str() {
                    "1" | "one" => StopBits::One,
                    "2" | "two" => StopBits::Two,
                    _ => return Err("Stop Bits option passed an invalid value"),
                })
            })
            .unwrap()
            .unwrap_or(dargs.stop_bits),

        timeout: pargs
            .opt_value_from_str(["-T", "--timeout"])
            .unwrap()
            .map(|t| Duration::from_millis(t))
            .unwrap_or(dargs.timeout),

        mode: pargs
            .opt_value_from_str(["-w", "--mode"])
            .unwrap()
            .unwrap_or(dargs.mode),

        capacity: pargs
            .opt_value_from_str(["-C", "--capacity"])
            .unwrap()
            .unwrap_or(dargs.capacity),

        output_file: pargs
            .opt_value_from_os_str::<_, _, &'static str>(["-o", "--output-file"], |s| {
                Ok(s.to_owned())
            })
            .unwrap()
            .unwrap_or(dargs.output_file),

        append: pargs
            .opt_value_from_str(["-a", "--append"])
            .unwrap()
            .unwrap_or(dargs.append),

        convertfrom: pargs
            .opt_value_from_str(["-c", "--convert"])
            .unwrap()
            .unwrap_or(dargs.convertfrom),
    };

    if pargs.contains(["-o", "--output-file"]) {
        todo!("File argument not supported yet")
    }
    if pargs.contains(["-f", "--format"]) {
        todo!("Parse format argument not supported yet")
    }

    let remaining = pargs.finish();
    if !remaining.is_empty() {
        eprintln!("Warning: unused arguments left: {:?}.", remaining);
    }

    let port = serialport::new(args.port.clone(), args.baud_rate)
        .data_bits(args.data_bits)
        .flow_control(args.flow_control)
        .parity(args.parity)
        .stop_bits(args.stop_bits)
        .timeout(args.timeout)
        .open()
        .expect("Could not open the serial port");

    match args.mode.to_lowercase().as_str() {
        "stdout" => serial_to_writer(port, stdout().lock(), false, &args),
        "iterm" => serial_iterm(port, &args),
        "lines" => serial_line_interactive(port, &args),
        "file" => serial_to_writer(
            port,
            File::with_options()
                .write(true)
                .append(true)
                .create(true)
                .open(args.output_file.as_os_str())
                .unwrap(),
            // File::open(args.output_file.as_os_str()).unwrap(),
            true,
            &args,
        ),
        _ => {
            println!("Invalid serial mode passed");
        }
    }
}

fn serial_line_interactive(port: Box<dyn SerialPort>, args: &Args) {
    todo!("Interactive mode not implemented yet")
}

fn serial_iterm(port: Box<dyn SerialPort>, args: &Args) {
    todo!("Terminal mode not implemented yet")
}

/// Read only streaming from the serial port
/// Writes the data to the Write object (buffered for performance)
fn serial_to_writer(mut port: Box<dyn SerialPort>, out: impl Write, counts: bool, args: &Args) {
    let mut out = BufWriter::with_capacity(args.capacity, out);
    let mut buf = Vec::with_capacity(args.capacity);
    let buf = &mut buf;

    let mut stamp = Instant::now();

    let mut count_words = 0;
    let mut count_commas = 0;
    let mut count_bytes = 0;
    let mut count_lines = 0;

    let mut copy = move || -> Result<_, _> {
        match port.read(buf) {
            Ok(n) => {
                count_bytes += n;
                match args.convertfrom {
                    ConvertFrom::NON => {
                        let (words, commas, lines) = buf.iter().fold(
                            (count_words, count_commas, count_lines),
                            |(w, c, l), &b| match b {
                                b' ' => (w + 1, c, l),
                                b'\n' => (w, c, l + 1),
                                b',' => (w, c + 1, l),
                                _ => (w, c, l),
                            },
                        );
                        count_words = words;
                        count_commas = commas;
                        count_lines = lines;
                    }
                    ConvertFrom::HEX => {
                        let mut out = Vec::with_capacity(buf.len() * 2);
                        for byte in &*buf {
                            write!(out, "{:x}", byte)?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::BIN => {
                        let mut out = Vec::with_capacity(buf.len() * 8);
                        for byte in &*buf {
                            write!(out, "{:b}", byte)?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::INT => {
                        let mut out = Vec::with_capacity(buf.len() * 4);
                        for &bytes in buf.array_chunks() {
                            write!(out, "{}", i32::from_le_bytes(bytes))?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::SHR => {
                        let mut out = Vec::with_capacity(buf.len() * 2);
                        for &bytes in buf.array_chunks() {
                            write!(out, "{}", i16::from_le_bytes(bytes))?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::FLT => {
                        let mut out = Vec::with_capacity(buf.len() * 4);
                        for &bytes in buf.array_chunks() {
                            // TODO: floating point decimal points??
                            write!(out, "{}", f32::from_le_bytes(bytes))?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::UINT => {
                        let mut out = Vec::with_capacity(buf.len() * 4);
                        for &bytes in buf.array_chunks() {
                            write!(out, "{}", u32::from_le_bytes(bytes))?;
                        }
                        *buf = out;
                    }
                    ConvertFrom::USHR => {
                        let mut out = Vec::with_capacity(buf.len() * 2);
                        for &bytes in buf.array_chunks() {
                            write!(out, "{}", u16::from_le_bytes(bytes))?;
                        }
                        *buf = out;
                    }
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => return Err(e),
        }

        out.write_all(&buf)?; // TODO: I think this should exit the program

        let now = Instant::now();
        let time = now - stamp;
        if time.as_secs() >= 1 {
            count_words = 0;
            count_commas = 0;
            count_bytes = 0;
            count_lines = 0;
            println!(
                "w{}, c{}, b{}, l{}",
                count_words as f64 / time.as_secs_f64(),
                count_commas as f64 / time.as_secs_f64(),
                count_bytes as f64 / time.as_secs_f64(),
                count_lines as f64 / time.as_secs_f64(),
            );
            stamp = now;
        }

        Ok(())
    };

    loop {
        match copy() {
            Ok(_) => {}
            Err(e) => serial_read_error(e),
        };
        // match io::copy(&mut port, &mut out) {
        //     Ok(n) => {
        //         if counts {
        //             // TODO: count bytes, bytes/sec, lines, etc
        //             println!("{}", n);
        //         }
        //     }
        //     Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
        //     Err(e) => serial_read_error(&port, e),
        // }
    }
}

fn serial_read_error(e: std::io::Error) {
    // TODO: better error messages
    eprintln!("{:?}", e);
    // TODO: Should exit/panic?
}
